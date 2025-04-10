use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{CustomMsg, Uint128, Uint256};

#[cw_serde]
pub struct InstantiateMsg {
    pub owners: Vec<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Purchase {
        purchaser: String,
        amount: Uint128,
    },
    Finalize {
        mint_amount: Uint128,
        distribute_amount: Uint128,
        pusd_amount: Uint128,
    },
    Refund {},
    SetPaloma {
        chain_id: String,
    },
    UpdateCompass {
        chain_id: String,
        new_compass: String,
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
pub enum PalomaMsg {
    /// Message struct for tokenfactory calls.
    TokenFactoryMsg {
        create_denom: Option<CreateDenomMsg>,
        mint_tokens: Option<MintMsg>,
    },
    SkywayMsg {
        set_erc20_to_denom: SetErc20ToDenom,
    },
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

impl CustomMsg for PalomaMsg {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}
