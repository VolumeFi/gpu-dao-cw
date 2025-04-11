#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, PalomaMsg, QueryMsg};
use crate::state::{State, STATE};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:gpu-dao-cw";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const CREATE_AMM_PAIR_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let mut state = State {
        pusd_denom: msg.pusd_denom,
        owners: msg
            .owners
            .iter()
            .map(|x| deps.api.addr_validate(x).unwrap())
            .collect(),
        finished: false,
        total_supply: Uint128::zero(),
    };

    if !state
        .owners
        .contains(&deps.api.addr_validate(&info.sender.to_string()).unwrap())
    {
        state
            .owners
            .push(deps.api.addr_validate(&info.sender.to_string()).unwrap());
    }

    STATE.save(deps.storage, &state)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new().add_attribute("action", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<PalomaMsg>, ContractError> {
    match msg {
        ExecuteMsg::Purchase { purchaser, amount } => {
            execute::purchase(deps, info, purchaser, amount)
        }
        ExecuteMsg::Finalize {
            palomadex_amm_factory,
            token_name,
            token_symbol,
            token_description,
            mint_amount,
            distribute_amount,
            pusd_amount,
        } => execute::finalize(
            deps,
            env,
            info,
            palomadex_amm_factory,
            token_name,
            token_symbol,
            token_description,
            mint_amount,
            distribute_amount,
            pusd_amount,
        ),
        ExecuteMsg::Claim { purchaser } => execute::claim(deps, env, info, purchaser),
        ExecuteMsg::Refund {} => execute::refund(deps),
        ExecuteMsg::SetPaloma { chain_id } => execute::set_paloma(deps, info, chain_id),
        ExecuteMsg::UpdateRefundWallet {
            chain_id,
            new_refund_wallet,
        } => execute::update_refund_wallet(deps, info, chain_id, new_refund_wallet),
        ExecuteMsg::UpdateGasFee {
            chain_id,
            new_gas_fee,
        } => execute::update_gas_fee(deps, info, chain_id, new_gas_fee),
        ExecuteMsg::UpdateServiceFeeCollector {
            chain_id,
            new_service_fee_collector,
        } => execute::update_service_fee_collector(deps, info, chain_id, new_service_fee_collector),
        ExecuteMsg::UpdateServiceFee {
            chain_id,
            new_service_fee,
        } => execute::update_service_fee(deps, info, chain_id, new_service_fee),
    }
}

pub mod execute {
    use std::collections::BTreeMap;

    use cosmwasm_std::{CosmosMsg, ReplyOn, SubMsg, Uint128, Uint256, WasmMsg};
    use ethabi::{Address, Contract, Function, Param, ParamType, StateMutability, Token, Uint};

    use super::*;
    use crate::{
        msg::{
            AssetInfo, CreateDenomMsg, DenomUnit, ExecuteJob, ExternalExecuteMsg, Metadata,
            MintMsg, PairType, PalomaMsg,
        },
        state::{CHAIN_SETTINGS, PURCHASE_LIST},
    };
    use std::str::FromStr;

    pub fn purchase(
        deps: DepsMut,
        info: MessageInfo,
        purchaser: String,
        amount: Uint128,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        let state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        assert!(
            state.finished == false,
            "The contract has already been finalized"
        );

        PURCHASE_LIST.update(deps.storage, purchaser, |old| -> StdResult<_> {
            Ok(old.unwrap_or_default() + amount)
        })?;

        Ok(Response::new().add_attribute("action", "purchase"))
    }

    pub fn finalize(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        palomadex_amm_factory: String,
        token_name: String,
        token_symbol: String,
        token_description: Option<String>,
        mint_amount: Uint128,
        distribute_amount: Uint128,
        pusd_amount: Uint128,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        let mut state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        assert!(
            state.finished == false,
            "The contract has already been finalized"
        );
        let denom_creator = env.contract.address.to_string();
        let subdenom = token_symbol.to_string();
        let denom = "factory/".to_string() + denom_creator.as_str() + "/" + subdenom.as_str();
        let metadata: Metadata = Metadata {
            description: token_description.unwrap_or_default(),
            denom_units: vec![
                DenomUnit {
                    denom: denom.clone(),
                    exponent: 0,
                    aliases: vec![],
                },
                DenomUnit {
                    denom: token_symbol.clone(),
                    exponent: 6,
                    aliases: vec![],
                },
            ],
            name: token_name.clone(),
            symbol: token_symbol.clone(),
            base: denom.clone(),
            display: token_symbol,
        };
        let messages = vec![
            CosmosMsg::Custom(PalomaMsg::TokenFactoryMsg {
                create_denom: Some(CreateDenomMsg {
                    subdenom: subdenom.to_string(),
                    metadata,
                }),
                mint_tokens: None,
            }),
            CosmosMsg::Custom(PalomaMsg::TokenFactoryMsg {
                create_denom: None,
                mint_tokens: Some(MintMsg {
                    denom: denom.clone(),
                    amount: mint_amount,
                    mint_to_address: denom_creator,
                }),
            }),
        ];
        let submessage = SubMsg {
            id: CREATE_AMM_PAIR_REPLY_ID,
            msg: WasmMsg::Execute {
                contract_addr: palomadex_amm_factory,
                msg: to_json_binary(&ExternalExecuteMsg::CreatePair {
                    pair_type: PairType::Xyk {},
                    asset_infos: vec![
                        AssetInfo::NativeToken { denom },
                        AssetInfo::NativeToken {
                            denom: state.pusd_denom,
                        },
                    ],
                    init_params: None,
                })?,
                funds: vec![],
            }
            .into(),
            gas_limit: None,
            reply_on: ReplyOn::Success,
            payload: todo!(),
        };
        state.finished = true;
        STATE.save(deps.storage, &state)?;
        Ok(Response::new()
            .add_messages(messages)
            .add_submessage(submessage)
            .add_attribute("action", "finalize"))
    }

    pub fn claim(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        purchaser: String,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        let state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        assert!(
            state.finished == true,
            "The contract has not been finalized yet"
        );
        let amount = PURCHASE_LIST.load(deps.storage, purchaser.clone())?;
        PURCHASE_LIST.remove(deps.storage, purchaser);
        Ok(Response::new().add_attribute("action", "claim"))
    }

    pub fn refund(deps: DepsMut) -> Result<Response<PalomaMsg>, ContractError> {
        Ok(Response::new().add_attribute("action", "refund"))
    }

    pub fn set_paloma(
        deps: DepsMut,
        info: MessageInfo,
        chain_id: String,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        // ACTION: Implement SetPaloma
        let state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );

        #[allow(deprecated)]
        let contract: Contract = Contract {
            constructor: None,
            functions: BTreeMap::from_iter(vec![(
                "set_paloma".to_string(),
                vec![Function {
                    name: "set_paloma".to_string(),
                    inputs: vec![],
                    outputs: Vec::new(),
                    constant: None,
                    state_mutability: StateMutability::NonPayable,
                }],
            )]),
            events: BTreeMap::new(),
            errors: BTreeMap::new(),
            receive: false,
            fallback: false,
        };
        Ok(Response::new()
            .add_message(CosmosMsg::Custom(PalomaMsg::SchedulerMsg {
                execute_job: ExecuteJob {
                    job_id: CHAIN_SETTINGS.load(deps.storage, chain_id.clone())?.job_id,
                    payload: Binary::new(
                        contract
                            .function("set_paloma")
                            .unwrap()
                            .encode_input(&[])
                            .unwrap(),
                    ),
                },
            }))
            .add_attribute("action", "set_paloma"))
    }

    pub fn update_refund_wallet(
        deps: DepsMut,
        info: MessageInfo,
        chain_id: String,
        new_refund_wallet: String,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        let state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        let update_refund_wallet_address: Address =
            Address::from_str(new_refund_wallet.as_str()).unwrap();
        #[allow(deprecated)]
        let contract: Contract = Contract {
            constructor: None,
            functions: BTreeMap::from_iter(vec![(
                "update_refund_wallet".to_string(),
                vec![Function {
                    name: "update_refund_wallet".to_string(),
                    inputs: vec![Param {
                        name: "new_refund_wallet".to_string(),
                        kind: ParamType::Address,
                        internal_type: None,
                    }],
                    outputs: Vec::new(),
                    constant: None,
                    state_mutability: StateMutability::NonPayable,
                }],
            )]),
            events: BTreeMap::new(),
            errors: BTreeMap::new(),
            receive: false,
            fallback: false,
        };
        Ok(Response::new()
            .add_message(CosmosMsg::Custom(PalomaMsg::SchedulerMsg {
                execute_job: ExecuteJob {
                    job_id: CHAIN_SETTINGS.load(deps.storage, chain_id.clone())?.job_id,
                    payload: Binary::new(
                        contract
                            .function("update_refund_wallet")
                            .unwrap()
                            .encode_input(&[Token::Address(update_refund_wallet_address)])
                            .unwrap(),
                    ),
                },
            }))
            .add_attribute("action", "update_refund_wallet"))
    }

    pub fn update_gas_fee(
        deps: DepsMut,
        info: MessageInfo,
        chain_id: String,
        new_gas_fee: Uint256,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        let state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        #[allow(deprecated)]
        let contract: Contract = Contract {
            constructor: None,
            functions: BTreeMap::from_iter(vec![(
                "update_gas_fee".to_string(),
                vec![Function {
                    name: "update_gas_fee".to_string(),
                    inputs: vec![Param {
                        name: "new_gas_fee".to_string(),
                        kind: ParamType::Uint(256),
                        internal_type: None,
                    }],
                    outputs: Vec::new(),
                    constant: None,
                    state_mutability: StateMutability::NonPayable,
                }],
            )]),
            events: BTreeMap::new(),
            errors: BTreeMap::new(),
            receive: false,
            fallback: false,
        };
        Ok(Response::new()
            .add_message(CosmosMsg::Custom(PalomaMsg::SchedulerMsg {
                execute_job: ExecuteJob {
                    job_id: CHAIN_SETTINGS.load(deps.storage, chain_id.clone())?.job_id,
                    payload: Binary::new(
                        contract
                            .function("update_gas_fee")
                            .unwrap()
                            .encode_input(&[Token::Uint(Uint::from_big_endian(
                                &new_gas_fee.to_be_bytes(),
                            ))])
                            .unwrap(),
                    ),
                },
            }))
            .add_attribute("action", "update_gas_fee"))
    }

    pub fn update_service_fee_collector(
        deps: DepsMut,
        info: MessageInfo,
        chain_id: String,
        new_service_fee_collector: String,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        let state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        let update_service_fee_collector_address: Address =
            Address::from_str(new_service_fee_collector.as_str()).unwrap();
        #[allow(deprecated)]
        let contract: Contract = Contract {
            constructor: None,
            functions: BTreeMap::from_iter(vec![(
                "update_service_fee_collector".to_string(),
                vec![Function {
                    name: "update_service_fee_collector".to_string(),
                    inputs: vec![Param {
                        name: "new_service_fee_collector".to_string(),
                        kind: ParamType::Address,
                        internal_type: None,
                    }],
                    outputs: Vec::new(),
                    constant: None,
                    state_mutability: StateMutability::NonPayable,
                }],
            )]),
            events: BTreeMap::new(),
            errors: BTreeMap::new(),
            receive: false,
            fallback: false,
        };
        Ok(Response::new()
            .add_message(CosmosMsg::Custom(PalomaMsg::SchedulerMsg {
                execute_job: ExecuteJob {
                    job_id: CHAIN_SETTINGS.load(deps.storage, chain_id.clone())?.job_id,
                    payload: Binary::new(
                        contract
                            .function("update_service_fee_collector")
                            .unwrap()
                            .encode_input(&[Token::Address(update_service_fee_collector_address)])
                            .unwrap(),
                    ),
                },
            }))
            .add_attribute("action", "update_service_fee_collector"))
    }

    pub fn update_service_fee(
        deps: DepsMut,
        info: MessageInfo,
        chain_id: String,
        new_service_fee: Uint256,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        let state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        #[allow(deprecated)]
        let contract: Contract = Contract {
            constructor: None,
            functions: BTreeMap::from_iter(vec![(
                "update_service_fee".to_string(),
                vec![Function {
                    name: "update_service_fee".to_string(),
                    inputs: vec![Param {
                        name: "new_service_fee".to_string(),
                        kind: ParamType::Uint(256),
                        internal_type: None,
                    }],
                    outputs: Vec::new(),
                    constant: None,
                    state_mutability: StateMutability::NonPayable,
                }],
            )]),
            events: BTreeMap::new(),
            errors: BTreeMap::new(),
            receive: false,
            fallback: false,
        };
        Ok(Response::new()
            .add_message(CosmosMsg::Custom(PalomaMsg::SchedulerMsg {
                execute_job: ExecuteJob {
                    job_id: CHAIN_SETTINGS.load(deps.storage, chain_id.clone())?.job_id,
                    payload: Binary::new(
                        contract
                            .function("update_service_fee")
                            .unwrap()
                            .encode_input(&[Token::Uint(Uint::from_big_endian(
                                &new_service_fee.to_be_bytes(),
                            ))])
                            .unwrap(),
                    ),
                },
            }))
            .add_attribute("action", "update_service_fee"))
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    unimplemented!()
    // match msg {
    //     QueryMsg::GetCount {} => to_json_binary(&query::count(deps)?),
    // }
}

pub mod query {
    // use super::*;

    // pub fn count(deps: Deps) -> StdResult<GetCountResponse> {
    //     let state = STATE.load(deps.storage)?;
    //     Ok(GetCountResponse { count: state.count })
    // }
}
