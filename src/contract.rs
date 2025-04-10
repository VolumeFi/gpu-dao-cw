#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{State, STATE};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:gpu-dao-cw";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let mut state = State {
        owners: msg
            .owners
            .iter()
            .map(|x| deps.api.addr_validate(x).unwrap())
            .collect(),
        finished: false,
    };

    if !state.owners.contains(&deps.api.addr_validate(&info.sender.to_string()).unwrap()) {
        state.owners.push(deps.api.addr_validate(&info.sender.to_string()).unwrap());
    }
    
    STATE.save(deps.storage, &state)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new().add_attribute("action", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Purchase{purchaser,amount}=>execute::purchase(deps,info,purchaser,amount),
        ExecuteMsg::Finalize{mint_amount,distribute_amount,pusd_amount,}=>execute::finalize(deps,info,mint_amount,distribute_amount,pusd_amount),
        ExecuteMsg::Refund{}=>execute::refund(deps),
        ExecuteMsg::SetPaloma { chain_id } => execute::set_paloma(deps, chain_id),
        ExecuteMsg::UpdateCompass { chain_id, new_compass } => 
            execute::update_compass(deps, chain_id, new_compass),
        ExecuteMsg::UpdateRefundWallet { chain_id, new_refund_wallet } => 
            execute::update_refund_wallet(deps, chain_id, new_refund_wallet),
        ExecuteMsg::UpdateGasFee { chain_id, new_gas_fee } => 
            execute::update_gas_fee(deps, chain_id, new_gas_fee),
        ExecuteMsg::UpdateServiceFeeCollector { chain_id, new_service_fee_collector } => 
            execute::update_service_fee_collector(deps, chain_id, new_service_fee_collector),
        ExecuteMsg::UpdateServiceFee { chain_id, new_service_fee } => 
            execute::update_service_fee(deps, chain_id, new_service_fee),
    }
}

pub mod execute {
    use std::collections::BTreeMap;

    use cosmwasm_std::{CosmosMsg, Uint128, Uint256};
    use ethabi::{Address, Contract, Function, Param, ParamType, StateMutability, Token, Uint};

    use crate::{msg::{CreateDenomMsg, MintMsg, PalomaMsg}, state::PURCHASE_LIST};
    use std::str::FromStr;
    use super::*;

    pub fn purchase(
        deps: DepsMut,
        info: MessageInfo,
        purchaser: String,
        amount: Uint128,
    ) -> Result<Response, ContractError> {
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
        info: MessageInfo,
        mint_amount: Uint128,
        distribute_amount: Uint128,
        pusd_amount: Uint128,
    ) -> Result<Response, ContractError> {
        let mut state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        assert!(
            state.finished == false,
            "The contract has already been finalized"
        );
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
        state.finished = true;
        STATE.save(deps.storage, &state)?;
        Ok(Response::new().add_attribute("action", "finalize"))
    }

    pub fn refund(deps: DepsMut) -> Result<Response, ContractError> {
        Ok(Response::new().add_attribute("action", "refund"))
    }

    pub fn set_paloma(deps: DepsMut, chain_id: String) -> Result<Response, ContractError> {
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
                    job_id: CHAIN_SETTINGS
                        .load(deps.storage, chain_id.clone())?
                        .main_job_id,
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

    pub fn update_compass(
        deps: DepsMut,
        chain_id: String,
        new_compass: String,
    ) -> Result<Response, ContractError> {
        let state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );

        #[allow(deprecated)]
        let contract: Contract = Contract {
            constructor: None,
            functions: BTreeMap::from_iter(vec![(
                "update_compass".to_string(),
                vec![Function {
                    name: "update_compass".to_string(),
                    inputs: vec![Param {
                        name: "new_compass".to_string(),
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
        let tokens = &[Token::Address(
            Address::from_str(new_compass.as_str()).unwrap(),
        )];
        Ok(Response::new()
            .add_message(CosmosMsg::Custom(PalomaMsg::SchedulerMsg {
                execute_job: ExecuteJob {
                    job_id: CHAIN_SETTINGS
                        .load(deps.storage, chain_id.clone())?
                        .main_job_id,
                    payload: Binary::new(
                        contract
                            .function("update_compass")
                            .unwrap()
                            .encode_input(tokens)
                            .unwrap(),
                    ),
                },
            }))
            .add_attributes(vec![
                ("action", "update_compass"),
                ("chain_id", &chain_id),
                ("new_compass", new_compass.as_str()),
            ]))
    }

    pub fn update_refund_wallet(
        deps: DepsMut,
        chain_id: String,
        new_refund_wallet: String,
    ) -> Result<Response, ContractError> {
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
                    job_id: CHAIN_SETTINGS
                        .load(deps.storage, chain_id.clone())?
                        .main_job_id,
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
        chain_id: String,
        new_gas_fee: Uint256,
    ) -> Result<Response, ContractError> {
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
                    job_id: CHAIN_SETTINGS
                        .load(deps.storage, chain_id.clone())?
                        .main_job_id,
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
        chain_id: String,
        new_service_fee_collector: String,
    ) -> Result<Response, ContractError> {
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
                    job_id: CHAIN_SETTINGS
                        .load(deps.storage, chain_id.clone())?
                        .main_job_id,
                    payload: Binary::new(
                        contract
                            .function("update_service_fee_collector")
                            .unwrap()
                            .encode_input(&[Token::Address(
                                update_service_fee_collector_address,
                            )])
                            .unwrap(),
                    ),
                },
            }))
            .add_attribute("action", "update_service_fee_collector"))
    }
    
    pub fn update_service_fee(
        deps: DepsMut,
        chain_id: String,
        new_service_fee: Uint256,
    ) -> Result<Response, ContractError> {
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
                    job_id: CHAIN_SETTINGS
                        .load(deps.storage, chain_id.clone())?
                        .main_job_id,
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
