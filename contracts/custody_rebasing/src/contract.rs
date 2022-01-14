#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, from_binary, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult,
};

use cosmwasm_bignumber::Uint256;

use crate::collateral::{
    deposit_collateral, liquidate_collateral, lock_collateral, query_borrower, query_borrowers,
    unlock_collateral, withdraw_collateral,
};
use crate::error::ContractError;
use crate::state::{
    read_config, read_current_rebase_index, save_current_rebase_index, store_config,
    update_total_cumulative_rewards, Config,
};

use cw20::Cw20ReceiveMsg;
use moneymarket::common::optional_addr_validate;
use moneymarket::custody::{Cw20HookMsg, ExecuteMsg, QueryMsg};
use moneymarket::custody_rebasing::{ConfigResponse, InstantiateMsg};
use moneymarket::oracle::PriceResponse;
use moneymarket::querier::{query_price, query_token_balance};
use terra_cosmwasm::TerraMsgWrapper;

pub const CLAIM_REWARDS_OPERATION: u64 = 1u64;
pub const SWAP_TO_STABLE_OPERATION: u64 = 2u64;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let config = Config {
        owner: deps.api.addr_canonicalize(&msg.owner)?,
        oracle: deps.api.addr_canonicalize(&msg.owner)?,
        overseer_contract: deps.api.addr_canonicalize(&msg.overseer_contract)?,
        collateral_token: deps.api.addr_canonicalize(&msg.collateral_token)?,
        underlying_token: deps.api.addr_canonicalize(&msg.collateral_token)?,
        market_contract: deps.api.addr_canonicalize(&msg.market_contract)?,
        liquidation_contract: deps.api.addr_canonicalize(&msg.liquidation_contract)?,
        stable_denom: msg.stable_denom,
        basset_info: msg.basset_info,
    };

    store_config(deps.storage, &config)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, info, msg),
        ExecuteMsg::UpdateConfig {
            owner,
            liquidation_contract,
        } => {
            let api = deps.api;
            update_config(
                deps,
                info,
                optional_addr_validate(api, owner)?,
                optional_addr_validate(api, liquidation_contract)?,
            )
        }
        ExecuteMsg::LockCollateral { borrower, amount } => {
            let borrower_addr = deps.api.addr_validate(&borrower)?;
            lock_collateral(deps, info, borrower_addr, amount)
        }
        ExecuteMsg::UnlockCollateral { borrower, amount } => {
            let borrower_addr = deps.api.addr_validate(&borrower)?;
            unlock_collateral(deps, info, borrower_addr, amount)
        }
        ExecuteMsg::DistributeRewards {} => Err(ContractError::RewardDistributionNotSupported {}),
        ExecuteMsg::WithdrawCollateral { amount } => withdraw_collateral(deps, info, amount),
        ExecuteMsg::LiquidateCollateral {
            liquidator,
            borrower,
            amount,
        } => {
            let liquidator_addr = deps.api.addr_validate(&liquidator)?;
            let borrower_addr = deps.api.addr_validate(&borrower)?;
            liquidate_collateral(deps, info, liquidator_addr, borrower_addr, amount)
        }
    }
}

pub fn receive_cw20(
    mut deps: DepsMut,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let contract_addr = info.sender.clone();

    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::DepositCollateral {}) => {
            // only asset contract can execute this message
            let config: Config = read_config(deps.storage)?;
            if deps.api.addr_canonicalize(contract_addr.as_str())? != config.collateral_token {
                return Err(ContractError::Unauthorized {});
            }

            let cw20_sender_addr = deps.api.addr_validate(&cw20_msg.sender)?;
            let total_collateral_amount =
                query_token_balance(deps.as_ref(), info.sender.clone(), cw20_sender_addr.clone())?
                    - cw20_msg.amount.clone().into();
            update_rebasing_rewards(&mut deps, &config, total_collateral_amount)?;
            deposit_collateral(deps, cw20_sender_addr, cw20_msg.amount.into())
        }
        _ => Err(ContractError::MissingDepositCollateralHook {}),
    }
}

pub fn update_rebasing_rewards(
    deps: &mut DepsMut,
    config: &Config,
    total_collateral_amount: Uint256,
) -> Result<(), ContractError> {
    let oracle_address = deps.api.addr_humanize(&config.oracle)?;

    let collateral_token = deps
        .api
        .addr_humanize(&config.collateral_token)?
        .to_string();
    let underlying_token = deps
        .api
        .addr_humanize(&config.underlying_token)?
        .to_string();

    let collateral_price: PriceResponse = query_price(
        deps.as_ref(),
        oracle_address.clone(),
        collateral_token,
        config.stable_denom.clone(),
        None,
    )?;

    let underlying_price: PriceResponse = query_price(
        deps.as_ref(),
        oracle_address,
        underlying_token,
        config.stable_denom.clone(),
        None,
    )?;

    let new_rebase_index = collateral_price.rate / underlying_price.rate;
    let old_rebase_index = read_current_rebase_index(deps.storage).unwrap_or(new_rebase_index);
    let reward_in_stable_denom =
        (new_rebase_index - old_rebase_index) * underlying_price.rate * total_collateral_amount;
    update_total_cumulative_rewards(deps.storage, &reward_in_stable_denom)?;
    save_current_rebase_index(deps.storage, &new_rebase_index)?;
    Ok(())
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<Addr>,
    liquidation_contract: Option<Addr>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let mut config: Config = read_config(deps.storage)?;

    if deps.api.addr_canonicalize(info.sender.as_str())? != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(owner) = owner {
        config.owner = deps.api.addr_canonicalize(owner.as_str())?;
    }

    if let Some(liquidation_contract) = liquidation_contract {
        config.liquidation_contract = deps.api.addr_canonicalize(liquidation_contract.as_str())?;
    }

    store_config(deps.storage, &config)?;
    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::Borrower { address } => {
            let addr = deps.api.addr_validate(&address)?;
            to_binary(&query_borrower(deps, addr)?)
        }
        QueryMsg::Borrowers { start_after, limit } => to_binary(&query_borrowers(
            deps,
            optional_addr_validate(deps.api, start_after)?,
            limit,
        )?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = read_config(deps.storage)?;
    Ok(ConfigResponse {
        owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        oracle: deps.api.addr_humanize(&config.oracle)?.to_string(),
        collateral_token: deps
            .api
            .addr_humanize(&config.collateral_token)?
            .to_string(),
        underlying_token: deps
            .api
            .addr_humanize(&config.underlying_token)?
            .to_string(),
        overseer_contract: deps
            .api
            .addr_humanize(&config.overseer_contract)?
            .to_string(),
        market_contract: deps.api.addr_humanize(&config.market_contract)?.to_string(),
        liquidation_contract: deps
            .api
            .addr_humanize(&config.liquidation_contract)?
            .to_string(),
        stable_denom: config.stable_denom,
        basset_info: config.basset_info,
    })
}
