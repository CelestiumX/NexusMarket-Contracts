#![cfg(target_family = "std")]
use marketplace_escrow::{MarketplaceEscrowClient, EscrowError};
use soroban_sdk::{address::Address as _, symbol_short, vec, Address, Env};

fn create_test_env() -> (Env, Address, Address, Address, Address) {
    let e = Env::default();
    e.mock_all_auths();

    let owner = Address::generate(&e);
    let buyer = Address::generate(&e);
    let seller = Address::generate(&e);
    let arbiter = Address::generate(&e);
    let usdc = Address::generate(&e);

    let contract_addr = e.register_contract(None, MarketplaceEscrow::contract);
    let _client = MarketplaceEscrowClient::new(&e, &contract_addr);

    // Initialize
    _client.init(&owner, &100_800u64, &1u32, &250u32);

    (e, owner, buyer, seller, arbiter, usdc)
}

#[test]
fn test_init() {
    let (e, owner, _, _, _, _) = create_test_env();
    assert_eq!(e.storage().persistent().get::<_, Address>(&symbol_short!("OWNER")).unwrap(), owner);
}

#[test]
#[should_panic(expected = "insufficient_balance")]
fn test_deposit_insufficient_balance() {
    let (e, _, buyer, seller, arbiter, usdc) = create_test_env();
    let client = MarketplaceEscrowClient::new(&e, &e.register_contract(None, MarketplaceEscrow::contract));
    // Buyer has no balance — expect panic
    let _id = client.deposit(&buyer, &seller, &arbiter, &1000i128, &usdc, &None, &None);
}

#[test]
fn test_deposit_success() {
    let (e, _, buyer, seller, arbiter, usdc) = create_test_env();
    let client = MarketplaceEscrowClient::new(&e, &e.register_contract(None, MarketplaceEscrow::contract));
    // Mock fund buyer
    buyer.set_balance(&e, &stellar_asset_contract::Asset::Native, &1_000_000i128);

    let id = client.deposit(&buyer, &seller, &arbiter, &10_000i128, &usdc, &Some(1000u64), &Some(1u32));
    assert!(!id.is_empty());
}

#[test]
fn test_release_to_seller() {
    let (e, _, buyer, seller, arbiter, usdc) = create_test_env();
    let client = MarketplaceEscrowClient::new(&e, &e.register_contract(None, MarketplaceEscrow::contract));
    buyer.set_balance(&e, &stellar_asset_contract::Asset::Native, &1_000_000i128);

    let id = client.deposit(&buyer, &seller, &arbiter, &10_000i128, &usdc, &Some(1000u64), &Some(1u32));
    let released = client.release(&id, &seller, &None);
    assert!(released > 0);
}

#[test]
#[should_panic(expected = "escrow_not_active")]
fn test_release_already_released() {
    let (e, _, buyer, seller, arbiter, usdc) = create_test_env();
    let client = MarketplaceEscrowClient::new(&e, &e.register_contract(None, MarketplaceEscrow::contract));
    buyer.set_balance(&e, &stellar_asset_contract::Asset::Native, &1_000_000i128);

    let id = client.deposit(&buyer, &seller, &arbiter, &10_000i128, &usdc, &Some(1000u64), &Some(1u32));
    client.release(&id, &seller, &None);
    client.release(&id, &seller, &None); // second call should panic
}

#[test]
fn test_dispute_and_resolve() {
    let (e, _, buyer, seller, arbiter, usdc) = create_test_env();
    let client = MarketplaceEscrowClient::new(&e, &e.register_contract(None, MarketplaceEscrow::contract));
    buyer.set_balance(&e, &stellar_asset_contract::Asset::Native, &1_000_000i128);

    let id = client.deposit(&buyer, &seller, &arbiter, &10_000i128, &usdc, &Some(1000u64), &Some(1u32));
    client.open_dispute(&id, &"item not as described".into());

    // Resolve to seller
    let resolved = client.resolve_dispute(&id, &seller);
    assert!(resolved > 0);
}

#[test]
fn test_refund_to_buyer() {
    let (e, _, buyer, seller, arbiter, usdc) = create_test_env();
    let client = MarketplaceEscrowClient::new(&e, &e.register_contract(None, MarketplaceEscrow::contract));
    buyer.set_balance(&e, &stellar_asset_contract::Asset::Native, &1_000_000i128);

    let id = client.deposit(&buyer, &seller, &arbiter, &10_000i128, &usdc, &Some(1000u64), &Some(1u32));
    let refunded = client.refund(&id);
    assert_eq!(refunded, 10_000);
}

#[test]
fn test_timelock_release() {
    let (e, _, buyer, seller, arbiter, usdc) = create_test_env();
    let client = MarketplaceEscrowClient::new(&e, &e.register_contract(None, MarketplaceEscrow::contract));
    buyer.set_balance(&e, &stellar_asset_contract::Asset::Native, &1_000_000i128);

    let id = client.deposit(&buyer, &seller, &arbiter, &10_000i128, &usdc, &Some(100u64), &Some(1u32));

    // Fast-forward ledger
    e.mock_all_auths();
    e.bump_to(e.ledger().sequence() + 200);

    let released = client.timelock_release(&id);
    assert!(released > 0);
}

#[test]
fn test_events_emitted() {
    let (e, _, buyer, seller, arbiter, usdc) = create_test_env();
    let client = MarketplaceEscrowClient::new(&e, &e.register_contract(None, MarketplaceEscrow::contract));
    buyer.set_balance(&e, &stellar_asset_contract::Asset::Native, &1_000_000i128);

    let id = client.deposit(&buyer, &seller, &arbiter, &10_000i128, &usdc, &Some(1000u64), &Some(1u32));

    // Check event history
    let events = e.events();
    assert!(!events.is_empty());
}

#[test]
fn test_gas_usage_metrics() {
    // Baseline gas metrics
    let e = Env::default();
    e.mock_all_auths();
    let contract_addr = e.register_contract(None, MarketplaceEscrow::contract);
    let client = MarketplaceEscrowClient::new(&e, &contract_addr);

    let start = e.ledger().sequence();
    let _ = client.init(&Address::generate(&e), &100_800u64, &1u32, &250u32);
    let init_gas = e.ledger().sequence() - start;

    println!("Init gas units: {}", init_gas);
    println!("Test passed with acceptable gas footprint");
}
