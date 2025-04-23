use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdResult};
use crate::msg::ExecuteMsg;
use crate::state::{Service, SERVICES};

pub fn execute(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsg,
) -> StdResult<Response> {
    match _msg {
        ExecuteMsg::SaveService { name, description } => {
            let service = Service {
                name: name.clone(),
                description,
                owner: _info.sender,
            };
            SERVICES.save(_deps.storage, name, &service)?;
            Ok(Response::new().add_attribute("action", "save_service"))
        }
    }
}
