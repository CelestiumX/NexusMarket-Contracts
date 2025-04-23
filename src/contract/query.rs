use cosmwasm_std::{Deps, Env, StdResult};
use crate::msg::QueryMsg;
use crate::state::{Service, SERVICES};

pub fn query(
    deps: Deps,
    _env: Env,
    msg: QueryMsg,
) -> StdResult<Service> {
    match msg {
        QueryMsg::GetService { name } => SERVICES.load(deps.storage, name),
    }
}
