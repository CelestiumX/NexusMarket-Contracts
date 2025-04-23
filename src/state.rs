use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use cosmwasm_std::Addr;
use cw_storage_plus::Map;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Service {
    pub name: String,
    pub description: String,
    pub owner: Addr,
}

pub const SERVICES: Map<String, Service> = Map::new("services");