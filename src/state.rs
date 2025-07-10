use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

pub const VESTING_PERIOD: u64 = 60 * 60 * 24 * 365 * 2; // 2 years in seconds

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct State {
    pub owners: Vec<Addr>,
    pub governance: Addr,
    pub finished: bool,
    pub total_supply: Uint128,
    pub gpu_dao_denom: Option<String>,
    pub distribute_amount: Option<Uint128>,
    pub finalize_timestamp: Option<u64>,
    pub pusd_denom: String,
    pub palomadex_factory: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct ChainSetting {
    pub job_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct VestingInfo {
    pub last_timestamp: u64,
    pub amount: Uint128,
}

pub const STATE: Item<State> = Item::new("state");
pub const PURCHASE_LIST: Map<String, VestingInfo> = Map::new("purchase_list");
pub const CHAIN_SETTINGS: Map<String, ChainSetting> = Map::new("chain_settings");
