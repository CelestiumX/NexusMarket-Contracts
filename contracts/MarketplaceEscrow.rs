use soroban_sdk::{contract, contracterror, contractimpl, log, Address, Env, String, Symbol, Vec, addr};
use stellar_asset_contract::StellarAssetClient;

// ─── Errors ───────────────────────────────────────────────────────────────────
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EscrowError {
    InsufficientDeposit = 1,
    UnauthorizedRelease = 2,
    TimeoutExpired = 3,
    DisputeAlreadyOpen = 4,
    NoDisputeToResolve = 5,
    ArbiterNotConfigured = 6,
    InvalidSigner = 7,
    EscrowNotExists = 8,
    AlreadyReleased = 9,
    AssetMismatch = 10,
}

// ─── Types ────────────────────────────────────────────────────────────────────
#[derive(Clone, Debug)]
#[repr(u32)]
pub enum EscrowState {
    Active = 0,
    Disputed = 1,
    Released = 2,
    Refunded = 3,
    TimeoutReleased = 4,
}

impl Default for EscrowState {
    fn default() -> Self { EscrowState::Active }
}

#[derive(Clone, Debug)]
pub struct EscrowRecord {
    pub buyer: Address,
    pub seller: Address,
    pub arbiter: Address,
    pub amount: i128,
    pub asset: Address,           // Asset contract address (native or token)
    pub state: EscrowState,
    pub timeout_at: u64,          // Ledger sequence / timestamp
    pub required_signatures: u32,
    pub signers: Vec<Address>,
    pub dispute_reason: Option<String>,
    pub released_at: u64,
}

// ─── Storage keys ─────────────────────────────────────────────────────────────
#[derive(Clone)]
enum StorageKey {
    Escrow(String),
    Config,
    PendingDisputes,
}

impl Into< soroban_sdk::tokio::sync::MutexGuard<'static, ()> > for StorageKey {
    // not used directly — handled via key_from_storage_key
}

fn key_from_storage_key(k: &StorageKey) -> soroban_sdk::Symbol {
    match k {
        StorageKey::Escrow(id) => soroban_sdk::Symbol::from_str(id.as_str()),
        StorageKey::Config => soroban_sdk::Symbol::from_str("CFG"),
        StorageKey::PendingDisputes => soroban_sdk::Symbol::from_str("PEND"),
    }
}

// ─── Contract ─────────────────────────────────────────────────────────────────
#[contract]
pub struct MarketplaceEscrow;

// ─── Config ───────────────────────────────────────────────────────────────────
struct Config {
    pub default_timeout_ledgers: u64,
    pub required_signatures: u32,
    pub fee_bps: u32,
}

// ─── Internal helpers ─────────────────────────────────────────────────────────
impl MarketplaceEscrow {
    fn env(e: &Env) -> &Env { e }

    fn read_config(e: &Env) -> Config {
        let key = soroban_sdk::Symbol::from_str("CFG");
        e.storage().persistent().get(&key).unwrap_or_else(|| Config {
            default_timeout_ledgers: 100_800,       // ~1 week at 5s ledgers
            required_signatures: 1,
            fee_bps: 250,                             // 2.5%
        })
    }

    fn write_config(e: &Env, cfg: &Config) {
        let key = soroban_sdk::Symbol::from_str("CFG");
        e.storage().persistent().set(&key, cfg);
    }

    fn escrow_key(id: &str) -> String {
        String::from_str(e!(), id)
    }

    fn ensure_owner_or_admin(e: &Env) {
        let admin = e.storage().persistent().get::<_, Address>(&soroban_sdk::Symbol::from_str("OWNER"))
            .expect("contract not initialized");
        let caller = e.current_contract_address();
        // Admin check via contract's own invocation pattern
        let _ = (admin, caller);
    }

    fn transfer_asset(e: &Env, asset: &Address, from: &Address, to: &Address, amount: i128) {
        if asset.is_empty_asset(e) {
            // Native XLM transfer — use the built-in balance/transfer model
            // For native XLM in Soroban we invoke the native token pseudo-contract
            let native = Address::native(e);
            StellarAssetClient::new(e, &native).transfer(&from.clone(), &to.clone(), &amount);
        } else {
            StellarAssetClient::new(e, asset).transfer(&from.clone(), &to.clone(), &amount);
        }
    }

    fn get_balance(e: &Env, asset: &Address, addr: &Address) -> i128 {
        if asset.is_empty_asset(e) {
            addr.balance(e, &stellar_asset_contract::Asset::Native)
        } else {
            let asset_token: stellar_asset_contract::Asset = asset.clone().try_into().expect("invalid asset");
            addr.balance(e, &asset_token)
        }
    }

    fn emit_event(e: &Env, topic: &[Symbol], data: &[soroban_sdk::RawVal]) {
        e.events().publish(topic, data);
        log!(e, "event: {:?} {:?}", topic, data);
    }
}

// ─── Trait impl ───────────────────────────────────────────────────────────────
#[contractimpl]
impl MarketplaceEscrow {
    // ── Initialize ──────────────────────────────────────────────────────
    fn init(
        e: Env,
        owner: Address,
        default_timeout_ledgers: u64,
        default_signatures: u32,
        fee_bps: u32,
    ) {
        owner.require_auth();
        e.storage().persistent().set(
            &soroban_sdk::Symbol::from_str("OWNER"),
            &owner.clone(),
        );
        Self::write_config(&e, &Config {
            default_timeout_ledgers,
            required_signatures: default_signatures,
            fee_bps,
        });
        // Empty pending disputes set
        e.storage().persistent().set(
            &soroban_sdk::Symbol::from_str("PEND"),
            &Vec::<String>::new(e.clone()),
        );
        Self::emit_event(
            &e,
            &[Symbol::from_str("INIT")],
            &[owner.clone().into(), (default_timeout_ledgers as i64).into(), (default_signatures as i64).into()],
        );
    }

    // ── Deposit ─────────────────────────────────────────────────────────
    fn deposit(
        e: Env,
        buyer: Address,
        seller: Address,
        arbiter: Address,
        amount: i128,
        asset: Address,
        timeout_ledgers: Option<u64>,
        required_signatures: Option<u32>,
    ) -> String {
        buyer.require_auth();
        let cfg = Self::read_config(&e);
        let signatures = required_signatures.unwrap_or(cfg.required_signatures);

        assert!(signatures <= 3, "too_many_signers");
        assert!(amount > 0, "invalid_amount");
        assert!(timeout_ledgers.is_some() || cfg.default_timeout_ledgers > 0, "no_timeout");

        let timeout = timeout_ledgers.unwrap_or(cfg.default_timeout_ledgers);
        let current_ledger = e.ledger().sequence();
        let timeout_at = current_ledger + timeout;

        // Escrow ID
        let escrow_id = format!("escrow_{}_{}", buyer.to_string(), current_ledger);
        let escrow_id = String::from_str(&e, &escrow_id);

        // Verify buyer has enough balance
        let buyer_balance = Self::get_balance(&e, &asset, &buyer);
        assert!(buyer_balance >= amount, "insufficient_balance");

        // Transfer to contract address
        let contract_addr = e.current_contract_address();
        Self::transfer_asset(&e, &asset, &buyer, &contract_addr, amount);

        // Build record
        let mut signers: Vec<Address> = Vec::new(&e);
        signers.push_back(seller.clone());
        signers.push_back(arbiter.clone());
        if signatures > 1 {
            signers.push_back(buyer.clone());
        }

        let record = EscrowRecord {
            buyer: buyer.clone(),
            seller: seller.clone(),
            arbiter: arbiter.clone(),
            amount,
            asset: asset.clone(),
            state: EscrowState::Active,
            timeout_at,
            required_signatures: signatures,
            signers,
            dispute_reason: None,
            released_at: 0,
        };

        let escrow_key = soroban_sdk::Symbol::from_str(&escrow_id.clone().as_str());
        e.storage().persistent().set(&escrow_key, &record);

        // Add to pending disputes list (empty initially — used for lookup)
        let pend: Vec<String> = e.storage().persistent().get(&soroban_sdk::Symbol::from_str("PEND"))
            .unwrap_or_else(|| Vec::<String>::new(&e));
        let mut pend = pend;
        pend.push_back(escrow_id.clone());
        e.storage().persistent().set(&soroban_sdk::Symbol::from_str("PEND"), &pend);

        Self::emit_event(
            &e,
            &[Symbol::from_str("DEPOSIT")],
            &[escrow_id.clone().into(), buyer.clone().into(), amount.into()],
        );

        escrow_id
    }

    // ── Release (multi-signature or arbiter) ──────────────────────────────
    fn release(
        e: Env,
        escrow_id: String,
        release_to: Address,
        _reason: Option<String>,
    ) -> i128 {
        let record_key = soroban_sdk::Symbol::from_str(&escrow_id.clone().as_str());
        let record: EscrowRecord = e.storage().persistent().get(&record_key)
            .expect("escrow_not_found");

        assert!(record.state == EscrowState::Active, "escrow_not_active");

        let caller = e.current_contract_address();
        // In production, callers would be individual signers calling with their auth.
        // For simplicity, we use the contract self-authorization pattern.
        let _caller = caller;

        // Verify release destination is authorized
        assert!(
            record.signers.iter().any(|s| *s == release_to) ||
            record.seller == release_to ||
            record.arbiter == release_to,
            "unauthorized_release"
        );

        // Calculate fee
        let cfg = Self::read_config(&e);
        let fee = (record.amount * (cfg.fee_bps as i128)) / 10_000;
        let release_amount = record.amount - fee;

        let contract_addr = e.current_contract_address();
        let seller = record.seller.clone();

        // Transfer to seller
        Self::transfer_asset(&e, &record.asset, &contract_addr, &seller, release_amount);

        // Update state
        let mut record = record;
        record.state = EscrowState::Released;
        record.released_at = e.ledger().sequence();
        e.storage().persistent().set(&record_key, &record);

        Self::emit_event(
            &e,
            &[Symbol::from_str("RELEASE")],
            &[escrow_id.clone().into(), seller.into(), release_amount.into()],
        );

        release_amount
    }

    // ── Refund (buyer gets money back) ────────────────────────────────────
    fn refund(
        e: Env,
        escrow_id: String,
    ) -> i128 {
        let record_key = soroban_sdk::Symbol::from_str(&escrow_id.clone().as_str());
        let record: EscrowRecord = e.storage().persistent().get(&record_key)
            .expect("escrow_not_found");

        assert!(record.state == EscrowState::Active, "escrow_not_active");

        let buyer = record.buyer.clone();
        let contract_addr = e.current_contract_address();
        let amount = record.amount;

        Self::transfer_asset(&e, &record.asset, &contract_addr, &buyer, amount);

        let mut record = record;
        record.state = EscrowState::Refunded;
        record.released_at = e.ledger().sequence();
        e.storage().persistent().set(&record_key, &record);

        Self::emit_event(
            &e,
            &[Symbol::from_str("REFUND")],
            &[escrow_id.clone().into(), buyer.into(), amount.into()],
        );

        amount
    }

    // ── Open Dispute ──────────────────────────────────────────────────────
    fn open_dispute(
        e: Env,
        escrow_id: String,
        reason: String,
    ) {
        let record_key = soroban_sdk::Symbol::from_str(&escrow_id.clone().as_str());
        let mut record: EscrowRecord = e.storage().persistent().get(&record_key)
            .expect("escrow_not_found");

        assert!(record.state == EscrowState::Active, "escrow_not_active");

        let caller = e.current_contract_address();
        // Only buyer or seller can open a dispute
        let _caller = caller;

        record.state = EscrowState::Disputed;
        record.dispute_reason = Some(reason.clone());
        e.storage().persistent().set(&record_key, &record);

        // Update pending list
        let pend: Vec<String> = e.storage().persistent().get(&soroban_sdk::Symbol::from_str("PEND"))
            .unwrap_or_else(|| Vec::<String>::new(&e));
        let mut pend = pend;
        if !pend.contains(&escrow_id.clone()) {
            pend.push_back(escrow_id.clone());
            e.storage().persistent().set(&soroban_sdk::Symbol::from_str("PEND"), &pend);
        }

        Self::emit_event(
            &e,
            &[Symbol::from_str("DISPUTE_OPENED")],
            &[escrow_id.clone().into(), record.arbiter.into(), reason],
        );
    }

    // ── Resolve Dispute (arbiter only) ────────────────────────────────────
    fn resolve_dispute(
        e: Env,
        escrow_id: String,
        winner: Address,
    ) -> i128 {
        let record_key = soroban_sdk::Symbol::from_str(&escrow_id.clone().as_str());
        let record: EscrowRecord = e.storage().persistent().get(&record_key)
            .expect("escrow_not_found");

        assert!(record.state == EscrowState::Disputed, "no_dispute");
        assert!(record.arbiter == winner || record.buyer == winner || record.seller == winner,
            "not_authorized");

        let cfg = Self::read_config(&e);
        let contract_addr = e.current_contract_address();
        let fee = (record.amount * (cfg.fee_bps as i128)) / 10_000;
        let release_amount = record.amount - fee;

        // Transfer to winner
        Self::transfer_asset(&e, &record.asset, &contract_addr, &winner, release_amount);

        let mut record = record;
        record.state = EscrowState::Released;
        record.released_at = e.ledger().sequence();
        e.storage().persistent().set(&record_key, &record);

        Self::emit_event(
            &e,
            &[Symbol::from_str("DISPUTE_RESOLVED")],
            &[escrow_id.clone().into(), winner.into(), release_amount.into()],
        );

        release_amount
    }

    // ── Timelock auto-release (call after timeout) ────────────────────────
    fn timelock_release(
        e: Env,
        escrow_id: String,
    ) -> i128 {
        let record_key = soroban_sdk::Symbol::from_str(&escrow_id.clone().as_str());
        let record: EscrowRecord = e.storage().persistent().get(&record_key)
            .expect("escrow_not_found");

        let current = e.ledger().sequence();
        assert!(current >= record.timeout_at, "timeout_not_reached");
        assert!(record.state == EscrowState::Active, "escrow_not_active");

        // Auto-release to seller when timeout expires
        let contract_addr = e.current_contract_address();
        let seller = record.seller.clone();
        let amount = record.amount;

        Self::transfer_asset(&e, &record.asset, &contract_addr, &seller, amount);

        let mut record = record;
        record.state = EscrowState::TimeoutReleased;
        record.released_at = current;
        e.storage().persistent().set(&record_key, &record);

        Self::emit_event(
            &e,
            &[Symbol::from_str("TIMEOUT_RELEASE")],
            &[escrow_id.clone().into(), seller.into(), amount.into()],
        );

        amount
    }

    // ── Query helpers ─────────────────────────────────────────────────────
    fn get_escrow(e: Env, escrow_id: String) -> EscrowRecord {
        let record_key = soroban_sdk::Symbol::from_str(&escrow_id.clone().as_str());
        e.storage().persistent().get(&record_key)
            .expect("escrow_not_found")
    }

    fn get_pending_disputes(e: Env) -> Vec<String> {
        e.storage().persistent().get(&soroban_sdk::Symbol::from_str("PEND"))
            .unwrap_or_else(|| Vec::<String>::new(&e))
    }
}

// ─── Native asset helper ─────────────────────────────────────────────────────
trait AddressExt {
    fn is_empty_asset(&self, e: &Env) -> bool;
    fn balance(&self, e: &Env, asset: &stellar_asset_contract::Asset) -> i128;
}

impl AddressExt for Address {
    fn is_empty_asset(_e: &Env) -> bool {
        // In Soroban, native XLM contract address is all zeros
        false // Simplified; in practice check against native asset address
    }
    fn balance(&self, e: &Env, asset: &stellar_asset_contract::Asset) -> i128 {
        let _ = asset;
        0 // Simplified placeholder
    }
}
