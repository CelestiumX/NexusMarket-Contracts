use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdResult};
use crate::msg::InstantiateMsg;

pub fn instantiate(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    Ok(Response::default())
}
