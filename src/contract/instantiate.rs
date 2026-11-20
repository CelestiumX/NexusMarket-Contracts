use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdResult, Addr, Uint128};
use crate::msg::InstantiateMsg;
use crate::state::{CONFIG, Config};

pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    // Validate the owner address
    let owner: Addr = deps.api.addr_validate(&msg.owner)?;

    // Build the contract configuration
    let config = Config {
        owner,
        fee: msg.fee,
    };

    // Persist the configuration
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}
