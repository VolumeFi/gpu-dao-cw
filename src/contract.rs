#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, from_json, to_json_binary, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Reply, Response, StdResult, SubMsgResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{
    Asset, AssetInfo, ExecuteMsg, ExternalExecuteMsg, ExternalQueryMsg, InstantiateMsg, PairInfo,
    PalomaMsg, QueryMsg,
};
use crate::state::{State, CHAIN_SETTINGS, PURCHASE_LIST, STATE};

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
        palomadex_factory: msg.palomadex_factory,
        owners: msg
            .owners
            .iter()
            .map(|x| deps.api.addr_validate(x).unwrap())
            .collect(),
        finished: false,
        total_supply: Uint128::zero(),
        gpu_dao_denom: None,
        governance: deps.api.addr_validate(&msg.governance).unwrap(),
        distribute_amount: None,
        finalize_timestamp: None,
    };

    if !state
        .owners
        .contains(&deps.api.addr_validate(info.sender.as_ref()).unwrap())
    {
        state
            .owners
            .push(deps.api.addr_validate(info.sender.as_ref()).unwrap());
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
        ExecuteMsg::SetBridge {
            erc20_address,
            chain_reference_id,
        } => execute::set_bridge(deps, info, erc20_address, chain_reference_id),
        ExecuteMsg::Claim { purchaser } => execute::claim(deps, env, info, purchaser),
        ExecuteMsg::Refund {
            chain_id,
            purchaser,
        } => execute::refund(deps, info, chain_id, purchaser),
        ExecuteMsg::SetChainSetting { chain_id, job_id } => {
            execute::set_chain_setting(deps, info, chain_id, job_id)
        }
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
            MintMsg, PairType, PalomaMsg, SendTx, SetErc20ToDenom,
        },
        state::{ChainSetting, VestingInfo, CHAIN_SETTINGS, PURCHASE_LIST, VESTING_PERIOD},
    };
    use std::str::FromStr;

    pub fn purchase(
        deps: DepsMut,
        info: MessageInfo,
        purchaser: String,
        amount: Uint128,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        let mut state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        assert!(!state.finished, "The contract has already been finalized");

        state.total_supply += amount;
        STATE.save(deps.storage, &state)?;

        let purchaser = deps.api.addr_validate(&purchaser)?;

        PURCHASE_LIST.update(deps.storage, purchaser.to_string(), |old| -> StdResult<_> {
            Ok(VestingInfo {
                last_timestamp: 0,
                amount: if let Some(old) = old {
                    old.amount + amount
                } else {
                    amount
                },
            })
        })?;

        Ok(Response::new().add_attribute("action", "purchase"))
    }

    #[allow(clippy::too_many_arguments)]
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
        assert!(!state.finished, "The contract has already been finalized");
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
        let payload = to_json_binary(&(denom.clone(), distribute_amount, pusd_amount))?;
        let submessage = SubMsg {
            id: CREATE_AMM_PAIR_REPLY_ID,
            msg: CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: palomadex_amm_factory,
                msg: to_json_binary(&ExternalExecuteMsg::CreatePair {
                    pair_type: PairType::Xyk {},
                    asset_infos: vec![
                        AssetInfo::NativeToken {
                            denom: denom.clone(),
                        },
                        AssetInfo::NativeToken {
                            denom: state.pusd_denom.clone(),
                        },
                    ],
                    init_params: None,
                })?,
                funds: vec![],
            }),
            gas_limit: None,
            reply_on: ReplyOn::Success,
            payload,
        };
        state.finished = true;
        state.finalize_timestamp = Some(env.block.time.seconds());
        state.gpu_dao_denom = Some(denom.clone());
        state.distribute_amount = Some(distribute_amount);
        STATE.save(deps.storage, &state)?;
        Ok(Response::new()
            .add_messages(messages)
            .add_submessage(submessage)
            .add_attribute("action", "finalize"))
    }

    pub fn set_bridge(
        deps: DepsMut,
        info: MessageInfo,
        erc20_address: String,
        chain_reference_id: String,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        let state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        assert!(
            state.gpu_dao_denom.is_some(),
            "The contract has not been finalized yet"
        );
        Ok(Response::new()
            .add_message(CosmosMsg::Custom(PalomaMsg::SkywayMsg {
                set_erc20_to_denom: Some(SetErc20ToDenom {
                    erc20_address,
                    token_denom: state.gpu_dao_denom.unwrap(),
                    chain_reference_id,
                }),
                send_tx: None,
            }))
            .add_attribute("action", "set_bridge"))
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
            state.finished && state.finalize_timestamp.is_some(),
            "The contract has not been finalized yet"
        );
        assert!(state.total_supply > Uint128::zero(), "No tokens to claim");
        let mut vesting_info = PURCHASE_LIST.load(deps.storage, purchaser.clone())?;
        assert!(vesting_info.amount > Uint128::zero(), "No tokens to claim");
        let current_timestamp =
            if env.block.time.seconds() < state.finalize_timestamp.unwrap() + VESTING_PERIOD {
                env.block.time.seconds()
            } else {
                state.finalize_timestamp.unwrap() + VESTING_PERIOD
            };

        if vesting_info.last_timestamp == 0 {
            vesting_info.last_timestamp = state.finalize_timestamp.unwrap();
        }
        let elapsed_time = current_timestamp - vesting_info.last_timestamp;

        let distribute_amount = state.distribute_amount.unwrap_or_default();

        let claimable_amount = if elapsed_time >= VESTING_PERIOD {
            vesting_info.amount
        } else {
            vesting_info.amount * Uint128::from(elapsed_time) / Uint128::from(VESTING_PERIOD)
        };
        let claimable_amount = distribute_amount * claimable_amount / state.total_supply;
        assert!(claimable_amount > Uint128::zero(), "No tokens to claim");
        vesting_info.last_timestamp = current_timestamp;
        PURCHASE_LIST.save(deps.storage, purchaser.clone(), &vesting_info)?;

        let claim_coin = Coin {
            denom: state.gpu_dao_denom.clone().unwrap(),
            amount: claimable_amount,
        };
        let message = CosmosMsg::Custom(PalomaMsg::SkywayMsg {
            set_erc20_to_denom: None,
            send_tx: Some(SendTx {
                remote_chain_destination_address: purchaser,
                amount: claim_coin.to_string(),
                chain_reference_id: state.gpu_dao_denom.clone().unwrap(),
            }),
        });

        Ok(Response::new()
            .add_message(message)
            .add_attribute("action", "claim"))
    }

    pub fn refund(
        deps: DepsMut,
        info: MessageInfo,
        chain_id: String,
        purchaser: String,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        let state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        assert!(!state.finished, "The contract has already been finalized");
        let vesting_info: VestingInfo = PURCHASE_LIST.load(deps.storage, purchaser.clone())?;
        assert!(vesting_info.amount > Uint128::zero(), "No tokens to refund");
        let refund_amount = vesting_info.amount;

        let send_coin = Coin {
            denom: state.gpu_dao_denom.clone().unwrap(),
            amount: refund_amount,
        };

        PURCHASE_LIST.remove(deps.storage, purchaser.clone());

        Ok(Response::new()
            .add_message(CosmosMsg::Custom(PalomaMsg::SkywayMsg {
                set_erc20_to_denom: None,
                send_tx: Some(SendTx {
                    remote_chain_destination_address: purchaser,
                    amount: send_coin.to_string(),
                    chain_reference_id: chain_id.clone(),
                }),
            }))
            .add_attribute("action", "refund"))
    }

    pub fn set_chain_setting(
        deps: DepsMut,
        info: MessageInfo,
        chain_id: String,
        job_id: String,
    ) -> Result<Response<PalomaMsg>, ContractError> {
        let state = STATE.load(deps.storage)?;
        assert!(
            state.owners.iter().any(|x| x == info.sender),
            "Unauthorized"
        );
        CHAIN_SETTINGS.save(
            deps.storage,
            chain_id.clone(),
            &ChainSetting {
                job_id: job_id.clone(),
            },
        )?;

        Ok(Response::new().add_attribute("action", "set_chain_setting"))
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

        let job_id = CHAIN_SETTINGS.load(deps.storage, chain_id.clone())?.job_id;

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
                    job_id,
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

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg {
        Reply {
            id: CREATE_AMM_PAIR_REPLY_ID,
            result: SubMsgResult::Ok(..),
            payload,
            gas_used: _,
        } => {
            let state = STATE.load(deps.storage)?;
            let (denom, distribute_amount, pusd_amount): (String, Uint128, Uint128) =
                from_json(payload)?;

            let pair_info: PairInfo = deps.querier.query_wasm_smart(
                state.palomadex_factory.to_string(),
                &ExternalQueryMsg::Pair {
                    asset_infos: vec![
                        AssetInfo::NativeToken {
                            denom: denom.clone(),
                        },
                        AssetInfo::NativeToken {
                            denom: state.pusd_denom.clone(),
                        },
                    ],
                },
            )?;
            let pair_contract = deps.api.addr_validate(pair_info.contract_addr.as_str())?;
            let mut gpu_dao_coin = deps
                .querier
                .query_balance(&env.contract.address, denom.clone())?;
            let mut pusd_coin = deps
                .querier
                .query_balance(&env.contract.address, state.pusd_denom.clone())?;
            gpu_dao_coin.amount -= distribute_amount;
            pusd_coin.amount -= pusd_amount;
            let funds = vec![
                gpu_dao_coin.clone(),
                Coin {
                    denom: state.pusd_denom.clone(),
                    amount: pusd_amount,
                },
            ];
            let messages: Vec<CosmosMsg> = vec![
                CosmosMsg::Bank(BankMsg::Send {
                    to_address: state.governance.to_string(),
                    amount: vec![pusd_coin],
                }),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: pair_contract.to_string(),
                    msg: to_json_binary(&ExternalExecuteMsg::ProvideLiquidity {
                        assets: vec![
                            Asset {
                                info: AssetInfo::NativeToken {
                                    denom: denom.clone(),
                                },
                                amount: gpu_dao_coin.amount,
                            },
                            Asset {
                                info: AssetInfo::NativeToken {
                                    denom: state.pusd_denom.clone(),
                                },
                                amount: pusd_amount,
                            },
                        ],
                        receiver: None,
                        slippage_tolerance: None,
                    })?,
                    funds: funds.clone(),
                }),
            ];
            Ok(Response::new().add_messages(messages).add_attributes(vec![
                attr("action", "finalize"),
                attr("pusd_amount", funds[0].amount.to_string()),
                attr("token_denom", funds[1].denom.clone()),
                attr("token_amount", funds[1].amount.to_string()),
            ]))
        }
        _ => Err(ContractError::FailedToParseReply {}),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::State {} => {
            let state = STATE.load(_deps.storage)?;
            to_json_binary(&state)
        }
        QueryMsg::PurchaseList { purchaser } => {
            let vesting_info = PURCHASE_LIST.load(_deps.storage, purchaser)?;
            to_json_binary(&vesting_info)
        }
        QueryMsg::ChainSettings { chain_id } => {
            let chain_setting = CHAIN_SETTINGS.load(_deps.storage, chain_id)?;
            to_json_binary(&chain_setting)
        }
    }
}
