#[allow(unused_imports)]
use crate::state::{ChainSetting, State, VestingInfo};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, CustomMsg, Decimal, Uint128, Uint256};

#[cw_serde]
pub struct InstantiateMsg {
    pub pusd_denom: String,
    pub governance: String,
    pub palomadex_factory: String,
    pub owners: Vec<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Purchase {
        purchaser: String,
        amount: Uint128,
    },
    Finalize {
        palomadex_amm_factory: String,
        token_name: String,
        token_symbol: String,
        token_description: Option<String>,
        mint_amount: Uint128,
        distribute_amount: Uint128,
        pusd_amount: Uint128,
    },
    SetBridge {
        erc20_address: String,
        chain_reference_id: String,
    },
    Claim {
        purchaser: String,
    },
    Refund {
        chain_id: String,
        purchaser: String,
    },
    SetPaloma {
        chain_id: String,
    },
    UpdateRefundWallet {
        chain_id: String,
        new_refund_wallet: String,
    },
    UpdateGasFee {
        chain_id: String,
        new_gas_fee: Uint256,
    },
    UpdateServiceFeeCollector {
        chain_id: String,
        new_service_fee_collector: String,
    },
    UpdateServiceFee {
        chain_id: String,
        new_service_fee: Uint256,
    },
}

#[cw_serde]
pub enum ExternalExecuteMsg {
    /// Palomadex Factory messages
    CreatePair {
        /// The pair type (exposed in [`PairType`])
        pair_type: PairType,
        /// The assets to create the pool for
        asset_infos: Vec<AssetInfo>,
        /// Optional binary serialised parameters for custom pool types
        init_params: Option<Binary>,
    },

    /// Palomadex Pair messages
    ProvideLiquidity {
        /// The assets available in the pool
        assets: Vec<Asset>,
        /// The slippage tolerance that allows liquidity provision only if the price in the pool doesn't move too much
        slippage_tolerance: Option<Decimal>,
        /// The receiver of LP tokens
        receiver: Option<String>,
    },
}

/// This enum describes a Terra asset (native or CW20).
#[cw_serde]
pub struct Asset {
    /// Information about an asset stored in a [`AssetInfo`] struct
    pub info: AssetInfo,
    /// A token amount
    pub amount: Uint128,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum ExternalQueryMsg {
    // PalomadexFactory queries
    #[returns(PairInfo)]
    Pair {
        /// The assets for which we return a pair
        asset_infos: Vec<AssetInfo>,
    },
}

/// This structure stores the main parameters for an palomadex pair
#[cw_serde]
pub struct PairInfo {
    /// Asset information for the assets in the pool
    pub asset_infos: Vec<AssetInfo>,
    /// Pair contract address
    pub contract_addr: Addr,
    /// Pair LP token address
    pub liquidity_token: Addr,
    /// The pool type (xyk, stableswap etc) available in [`PairType`]
    pub pair_type: PairType,
}

#[derive(Eq)]
#[cw_serde]
pub enum PairType {
    /// XYK pair type
    Xyk {},
    /// Stable pair type
    Stable {},
    /// Custom pair type
    Custom(String),
}

#[cw_serde]
#[derive(Hash, Eq)]
pub enum AssetInfo {
    /// Non-native Token
    Token { contract_addr: Addr },
    /// Native token
    NativeToken { denom: String },
}

#[cw_serde]
pub enum PalomaMsg {
    /// Message struct for cross-chain calls.
    SchedulerMsg { execute_job: ExecuteJob },
    /// Message struct for tokenfactory calls.
    TokenFactoryMsg {
        create_denom: Option<CreateDenomMsg>,
        mint_tokens: Option<MintMsg>,
    },
    SkywayMsg {
        set_erc20_to_denom: Option<SetErc20ToDenom>,
        send_tx: Option<SendTx>,
    },
}

#[cw_serde]
pub struct ExecuteJob {
    pub job_id: String,
    pub payload: Binary,
}

#[cw_serde]
pub struct CreateDenomMsg {
    pub subdenom: String,
    pub metadata: Metadata,
}

#[cw_serde]
pub struct Metadata {
    pub description: String,
    pub denom_units: Vec<DenomUnit>,
    pub base: String,
    pub display: String,
    pub name: String,
    pub symbol: String,
}

#[cw_serde]
pub struct DenomUnit {
    pub denom: String,
    pub exponent: u32,
    pub aliases: Vec<String>,
}

#[cw_serde]
pub struct MintMsg {
    pub denom: String,
    pub amount: Uint128,
    pub mint_to_address: String,
}

#[cw_serde]
pub struct SetErc20ToDenom {
    pub erc20_address: String,
    pub token_denom: String,
    pub chain_reference_id: String,
}

#[cw_serde]
pub struct SendTx {
    pub remote_chain_destination_address: String,
    pub amount: String,
    pub chain_reference_id: String,
}

impl CustomMsg for PalomaMsg {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Query the current state of the contract
    #[returns(State)]
    State {},

    /// Query the purchase list for a specific purchaser
    #[returns(VestingInfo)]
    PurchaseList { purchaser: String },

    #[returns(ChainSetting)]
    ChainSettings { chain_id: String },
}
