use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct State {
    pub owners: Vec<Addr>,
    pub finished: bool,
}

pub const STATE: Item<State> = Item::new("state");
pub const PURCHASE_LIST: Map<String, Uint128> = Map::new("purchase_list");
