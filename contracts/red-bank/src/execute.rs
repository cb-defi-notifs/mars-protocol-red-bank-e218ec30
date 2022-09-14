use std::str;

use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
    WasmMsg,
};
use cw20::{Cw20ExecuteMsg, MinterResponse};
use cw20_base::msg::InstantiateMarketingInfo;

use mars_outpost::address_provider::{self, MarsContract};
use mars_outpost::error::MarsError;
use mars_outpost::helpers::{
    build_send_asset_msg, cw20_get_balance, option_string_to_addr, zero_address,
};
use mars_outpost::red_bank::{
    Collateral, Config, CreateOrUpdateConfig, ExecuteMsg, InitOrUpdateAssetParams, InstantiateMsg,
    Market,
};
use mars_outpost::{ma_token, math};

use crate::error::ContractError;
use crate::health::{get_user_positions_health, is_above_max_ltv, is_liquidatable};
use crate::helpers::query_total_deposits;
use crate::interest_rates::{
    apply_accumulated_interests, get_scaled_debt_amount, get_scaled_liquidity_amount,
    get_underlying_debt_amount, get_underlying_liquidity_amount, update_interest_rates,
};
use crate::state::{
    user_is_borrowing, COLLATERALS, CONFIG, DEBTS, MARKETS, MARKET_DENOMS_BY_MA_TOKEN,
    UNCOLLATERALIZED_LOAN_LIMITS,
};

pub fn instantiate(deps: DepsMut, msg: InstantiateMsg) -> Result<Response, ContractError> {
    // Destructuring a struct’s fields into separate variables in order to force
    // compile error if we add more params
    let CreateOrUpdateConfig {
        owner,
        address_provider,
        ma_token_code_id,
        close_factor,
    } = msg.config;

    // All fields should be available
    let available = owner.is_some()
        && address_provider.is_some()
        && ma_token_code_id.is_some()
        && close_factor.is_some();

    if !available {
        return Err(MarsError::InstantiateParamsUnavailable {}.into());
    };

    let config = Config {
        owner: option_string_to_addr(deps.api, owner, zero_address())?,
        address_provider: option_string_to_addr(deps.api, address_provider, zero_address())?,
        ma_token_code_id: ma_token_code_id.unwrap(),
        close_factor: close_factor.unwrap(),
    };

    config.validate()?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

/// Update config
pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_config: CreateOrUpdateConfig,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(MarsError::Unauthorized {}.into());
    }

    // Destructuring a struct’s fields into separate variables in order to force
    // compile error if we add more params
    let CreateOrUpdateConfig {
        owner,
        address_provider,
        ma_token_code_id,
        close_factor,
    } = new_config;

    // Update config
    config.owner = option_string_to_addr(deps.api, owner, config.owner)?;
    config.address_provider =
        option_string_to_addr(deps.api, address_provider, config.address_provider)?;
    config.ma_token_code_id = ma_token_code_id.unwrap_or(config.ma_token_code_id);
    config.close_factor = close_factor.unwrap_or(config.close_factor);

    // Validate config
    config.validate()?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "outposts/red-bank/update_config"))
}

/// Initialize asset if not exist.
/// Initialization requires that all params are provided and there is no asset in state.
pub fn init_asset(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    denom: String,
    asset_params: InitOrUpdateAssetParams,
    asset_symbol_option: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(MarsError::Unauthorized {}.into());
    }

    if MARKETS.may_load(deps.storage, &denom)?.is_some() {
        return Err(ContractError::AssetAlreadyInitialized {});
    }

    let new_market = create_market(env.block.time.seconds(), &denom, asset_params)?;
    MARKETS.save(deps.storage, &denom, &new_market)?;

    let symbol = asset_symbol_option.unwrap_or_else(|| denom.clone());

    // Prepare response, should instantiate an maToken
    // and use the Register hook.
    // A new maToken should be created which callbacks this contract in order to be registered.
    let addresses = address_provider::helpers::query_addresses(
        deps.as_ref(),
        &config.address_provider,
        vec![MarsContract::Incentives, MarsContract::ProtocolAdmin],
    )?;
    // TODO: protocol admin may be a marshub address, which can't be validated into `Addr`
    let protocol_admin_addr = &addresses[&MarsContract::ProtocolAdmin];
    let incentives_addr = &addresses[&MarsContract::Incentives];

    let token_symbol = format!("ma{}", symbol);

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Instantiate {
            admin: Some(protocol_admin_addr.to_string()),
            code_id: config.ma_token_code_id,
            msg: to_binary(&ma_token::msg::InstantiateMsg {
                name: format!("Mars {} Liquidity Token", symbol),
                symbol: token_symbol.clone(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
                marketing: Some(InstantiateMarketingInfo {
                    project: Some(String::from("Mars Protocol")),
                    description: Some(format!(
                        "Interest earning token representing deposits for {}",
                        symbol
                    )),
                    marketing: Some(protocol_admin_addr.to_string()),
                    logo: None,
                }),
                init_hook: Some(ma_token::msg::InitHook {
                    contract_addr: env.contract.address.to_string(),
                    msg: to_binary(&ExecuteMsg::InitAssetTokenCallback {
                        denom: denom.clone(),
                    })?,
                }),
                red_bank_address: env.contract.address.to_string(),
                incentives_address: incentives_addr.into(),
            })?,
            funds: vec![],
            label: token_symbol,
        }))
        .add_attribute("action", "outposts/red-bank/init_asset")
        .add_attribute("denom", denom))
}

/// Initialize new market
pub fn create_market(
    block_time: u64,
    denom: &str,
    params: InitOrUpdateAssetParams,
) -> Result<Market, ContractError> {
    // Destructuring a struct’s fields into separate variables in order to force
    // compile error if we add more params
    let InitOrUpdateAssetParams {
        initial_borrow_rate: borrow_rate,
        max_loan_to_value,
        reserve_factor,
        liquidation_threshold,
        liquidation_bonus,
        interest_rate_model,
        deposit_enabled,
        borrow_enabled,
        deposit_cap,
    } = params;

    // All fields should be available
    let available = borrow_rate.is_some()
        && max_loan_to_value.is_some()
        && reserve_factor.is_some()
        && liquidation_threshold.is_some()
        && liquidation_bonus.is_some()
        && interest_rate_model.is_some()
        && deposit_enabled.is_some()
        && borrow_enabled.is_some();

    if !available {
        return Err(MarsError::InstantiateParamsUnavailable {}.into());
    }

    let new_market = Market {
        denom: denom.to_string(),
        ma_token_address: Addr::unchecked(""),
        borrow_index: Decimal::one(),
        liquidity_index: Decimal::one(),
        borrow_rate: borrow_rate.unwrap(),
        liquidity_rate: Decimal::zero(),
        max_loan_to_value: max_loan_to_value.unwrap(),
        reserve_factor: reserve_factor.unwrap(),
        indexes_last_updated: block_time,
        debt_total_scaled: Uint128::zero(),
        liquidation_threshold: liquidation_threshold.unwrap(),
        liquidation_bonus: liquidation_bonus.unwrap(),
        interest_rate_model: interest_rate_model.unwrap(),
        deposit_enabled: deposit_enabled.unwrap(),
        borrow_enabled: borrow_enabled.unwrap(),
        // if not specified, deposit cap is set to unlimited
        deposit_cap: deposit_cap.unwrap_or(Uint128::MAX),
    };

    new_market.validate()?;

    Ok(new_market)
}

pub fn init_asset_token_callback(
    deps: DepsMut,
    info: MessageInfo,
    denom: String,
) -> Result<Response, ContractError> {
    let mut market = MARKETS.load(deps.storage, &denom)?;

    if market.ma_token_address == zero_address() {
        let ma_contract_addr = info.sender;

        market.ma_token_address = ma_contract_addr.clone();
        MARKETS.save(deps.storage, &denom, &market)?;

        // save ma token contract to reference mapping
        MARKET_DENOMS_BY_MA_TOKEN.save(deps.storage, &ma_contract_addr, &denom)?;

        Ok(Response::new().add_attribute("action", "outposts/red-bank/init_asset_token_callback"))
    } else {
        // Can do this only once
        Err(MarsError::Unauthorized {}.into())
    }
}

/// Update asset with new params.
pub fn update_asset(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    denom: String,
    asset_params: InitOrUpdateAssetParams,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(MarsError::Unauthorized {}.into());
    }

    let market_option = MARKETS.may_load(deps.storage, &denom)?;
    match market_option {
        None => Err(ContractError::AssetNotInitialized {}),
        Some(mut market) => {
            // Destructuring a struct’s fields into separate variables in order to force
            // compile error if we add more params
            let InitOrUpdateAssetParams {
                initial_borrow_rate: _,
                max_loan_to_value,
                reserve_factor,
                liquidation_threshold,
                liquidation_bonus,
                interest_rate_model,
                deposit_enabled,
                borrow_enabled,
                deposit_cap,
            } = asset_params;

            // If reserve factor or interest rates are updated we update indexes with
            // current values before applying the change to prevent applying this
            // new params to a period where they were not valid yet. Interests rates are
            // recalculated after changes are applied.
            let should_update_interest_rates = (reserve_factor.is_some()
                && reserve_factor.unwrap() != market.reserve_factor)
                || interest_rate_model.is_some();

            let mut response = Response::new();

            if should_update_interest_rates {
                let protocol_rewards_collector_addr = address_provider::helpers::query_address(
                    deps.as_ref(),
                    &config.address_provider,
                    MarsContract::ProtocolRewardsCollector,
                )?;
                response = apply_accumulated_interests(
                    &env,
                    &protocol_rewards_collector_addr,
                    &mut market,
                    response,
                )?;
            }

            let mut updated_market = Market {
                max_loan_to_value: max_loan_to_value.unwrap_or(market.max_loan_to_value),
                reserve_factor: reserve_factor.unwrap_or(market.reserve_factor),
                liquidation_threshold: liquidation_threshold
                    .unwrap_or(market.liquidation_threshold),
                liquidation_bonus: liquidation_bonus.unwrap_or(market.liquidation_bonus),
                interest_rate_model: interest_rate_model.unwrap_or(market.interest_rate_model),
                deposit_enabled: deposit_enabled.unwrap_or(market.deposit_enabled),
                borrow_enabled: borrow_enabled.unwrap_or(market.borrow_enabled),
                deposit_cap: deposit_cap.unwrap_or(market.deposit_cap),
                ..market
            };

            updated_market.validate()?;

            if should_update_interest_rates {
                response = update_interest_rates(
                    &deps,
                    &env,
                    &mut updated_market,
                    Uint128::zero(),
                    &denom,
                    response,
                )?;
            }
            MARKETS.save(deps.storage, &denom, &updated_market)?;

            Ok(response
                .add_attribute("action", "outposts/red-bank/update_asset")
                .add_attribute("denom", &denom))
        }
    }
}

/// Update uncollateralized loan limit by a given amount in base asset
pub fn update_uncollateralized_loan_limit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    user_addr: Addr,
    denom: String,
    new_limit: Uint128,
) -> Result<Response, ContractError> {
    // Get config
    let config = CONFIG.load(deps.storage)?;

    // Only owner can do this
    if info.sender != config.owner {
        return Err(MarsError::Unauthorized {}.into());
    }

    if new_limit.is_zero() {
        UNCOLLATERALIZED_LOAN_LIMITS.remove(deps.storage, (&user_addr, &denom));
    } else {
        UNCOLLATERALIZED_LOAN_LIMITS.save(deps.storage, (&user_addr, &denom), &new_limit)?;
    }

    // The user's position must be healthy after updating the uncollateralized loan limit
    let oracle_addr = address_provider::helpers::query_address(
        deps.as_ref(),
        &config.address_provider,
        MarsContract::Oracle,
    )?;

    if is_liquidatable(deps.as_ref(), &env, &user_addr, &oracle_addr)? {
        return Err(ContractError::InvalidHealthFactorAfterSettingUncollateralizedLoanLimit {});
    }

    Ok(Response::new()
        .add_attribute("action", "outposts/red-bank/update_uncollateralized_loan_limit")
        .add_attribute("user", user_addr)
        .add_attribute("denom", denom)
        .add_attribute("new_allowance", new_limit))
}

/// Execute deposits and mint corresponding ma_tokens
pub fn deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    on_behalf_of: Option<String>,
    denom: String,
    deposit_amount: Uint128,
) -> Result<Response, ContractError> {
    let user_addr = if let Some(address) = on_behalf_of {
        deps.api.addr_validate(&address)?
    } else {
        info.sender.clone()
    };

    let mut market = MARKETS.load(deps.storage, &denom)?;
    if !market.deposit_enabled {
        return Err(ContractError::DepositNotEnabled {
            denom,
        });
    }

    let total_scaled_deposits = query_total_deposits(&deps.querier, &market.ma_token_address)?;
    let total_deposits =
        get_underlying_liquidity_amount(total_scaled_deposits, &market, env.block.time.seconds())?;
    if total_deposits.checked_add(deposit_amount)? > market.deposit_cap {
        return Err(ContractError::DepositCapExceeded {
            denom,
        });
    }

    // Update the user's collateral status
    // TODO: Currently, the logic is:
    // - If the user already has a collateral position, do nothing;
    // - If not, initialize a new one with `enable` default to `true`.
    // We don't increment the user's collateral shares here, as the collateral shares are tokenized
    // as maTokens.
    // Once maToken is removed, we increment the user's collateral shares here.
    COLLATERALS.update(deps.storage, (&user_addr, &denom), |collateral| -> StdResult<_> {
        Ok(collateral.unwrap_or(Collateral {
            enabled: true,
        }))
    })?;

    let mut response = Response::new();

    let config = CONFIG.load(deps.storage)?;

    // update indexes and interest rates
    let rewards_collector_addr = address_provider::helpers::query_address(
        deps.as_ref(),
        &config.address_provider,
        MarsContract::ProtocolRewardsCollector,
    )?;
    response = apply_accumulated_interests(&env, &rewards_collector_addr, &mut market, response)?;
    response = update_interest_rates(&deps, &env, &mut market, Uint128::zero(), &denom, response)?;
    MARKETS.save(deps.storage, &denom, &market)?;

    if market.liquidity_index.is_zero() {
        return Err(ContractError::InvalidLiquidityIndex {});
    }
    let mint_amount =
        get_scaled_liquidity_amount(deposit_amount, &market, env.block.time.seconds())?;

    Ok(response
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: market.ma_token_address.into(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: user_addr.to_string(),
                amount: mint_amount,
            })?,
            funds: vec![],
        }))
        .add_attribute("action", "outposts/red-bank/deposit")
        .add_attribute("denom", denom)
        .add_attribute("sender", info.sender)
        .add_attribute("user", user_addr)
        .add_attribute("amount", deposit_amount))
}

/// Burns sent maAsset in exchange of underlying asset
pub fn withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    denom: String,
    amount: Option<Uint128>,
    recipient: Option<String>,
) -> Result<Response, ContractError> {
    let withdrawer_addr = info.sender;

    let mut market = MARKETS.load(deps.storage, &denom)?;

    let asset_ma_addr = market.ma_token_address.clone();
    let withdrawer_balance_scaled_before =
        cw20_get_balance(&deps.querier, asset_ma_addr, withdrawer_addr.clone())?;

    if withdrawer_balance_scaled_before.is_zero() {
        return Err(ContractError::UserNoCollateralBalance {
            user: withdrawer_addr.into(),
            denom,
        });
    }

    let withdrawer_balance_before = get_underlying_liquidity_amount(
        withdrawer_balance_scaled_before,
        &market,
        env.block.time.seconds(),
    )?;

    let withdraw_amount = match amount {
        Some(amount) => {
            // Check user has sufficient balance to send back
            if amount.is_zero() || amount > withdrawer_balance_before {
                return Err(ContractError::InvalidWithdrawAmount {
                    denom,
                });
            };
            amount
        }
        None => {
            // If no amount is specified, the full balance is withdrawn
            withdrawer_balance_before
        }
    };

    let config = CONFIG.load(deps.storage)?;

    let addresses = address_provider::helpers::query_addresses(
        deps.as_ref(),
        &config.address_provider,
        vec![MarsContract::Oracle, MarsContract::ProtocolRewardsCollector],
    )?;
    let rewards_collector_addr = &addresses[&MarsContract::ProtocolRewardsCollector];
    let oracle_addr = &addresses[&MarsContract::Oracle];

    // NOTE: this load command currently doesn't work for the rewards collector, which doesn't have
    // a collateral position. however, this will be automatically solved in a later PR, once we
    // remove maToken and store collateral shares here in Red Bank.
    let collateral = COLLATERALS.load(deps.storage, (&withdrawer_addr, &denom))?;

    let mut response = Response::new();

    // update indexes and interest rates
    response = apply_accumulated_interests(&env, rewards_collector_addr, &mut market, response)?;
    response = update_interest_rates(&deps, &env, &mut market, withdraw_amount, &denom, response)?;
    MARKETS.save(deps.storage, &denom, &market)?;

    // burn maToken
    let withdrawer_balance_after = withdrawer_balance_before.checked_sub(withdraw_amount)?;
    let withdrawer_balance_scaled_after =
        get_scaled_liquidity_amount(withdrawer_balance_after, &market, env.block.time.seconds())?;

    let burn_amount =
        withdrawer_balance_scaled_before.checked_sub(withdrawer_balance_scaled_after)?;
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: market.ma_token_address.to_string(),
        msg: to_binary(&ma_token::msg::ExecuteMsg::Burn {
            user: withdrawer_addr.to_string(),
            amount: burn_amount,
        })?,
        funds: vec![],
    }));

    // if the user's maToken amount is reduced to zero, then delete the collateral position
    if withdrawer_balance_scaled_after.is_zero() {
        COLLATERALS.remove(deps.storage, (&withdrawer_addr, &denom));
    }

    // once market and user collateral has been updated, assert that the user's position is below
    // liquidation threshold
    // only check if the collateral was previously enabled, and the user is borrowing some assets
    if collateral.enabled
        && user_is_borrowing(deps.as_ref().storage, &withdrawer_addr)
        && is_liquidatable(deps.as_ref(), &env, &withdrawer_addr, oracle_addr)?
    {
        return Err(ContractError::InvalidHealthFactorAfterWithdraw {});
    }

    // send underlying asset to user or another recipient
    let recipient_addr = if let Some(recipient) = recipient {
        deps.api.addr_validate(&recipient)?
    } else {
        withdrawer_addr.clone()
    };

    Ok(response
        .add_message(build_send_asset_msg(&recipient_addr, &denom, withdraw_amount))
        .add_attribute("action", "outposts/red-bank/withdraw")
        .add_attribute("denom", denom)
        .add_attribute("user", withdrawer_addr)
        .add_attribute("recipient", recipient_addr)
        .add_attribute("burn_amount", burn_amount)
        .add_attribute("withdraw_amount", withdraw_amount))
}

/// Add debt for the borrower and send the borrowed funds
pub fn borrow(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    denom: String,
    borrow_amount: Uint128,
    recipient: Option<String>,
) -> Result<Response, ContractError> {
    let borrower_addr = info.sender;

    // Cannot borrow zero amount
    if borrow_amount.is_zero() {
        return Err(ContractError::InvalidBorrowAmount {
            denom,
        });
    }

    // Load market and user state
    let mut borrow_market = MARKETS.load(deps.storage, &denom)?;

    if !borrow_market.borrow_enabled {
        return Err(ContractError::BorrowNotEnabled {
            denom,
        });
    }

    let config = CONFIG.load(deps.storage)?;

    let addresses = address_provider::helpers::query_addresses(
        deps.as_ref(),
        &config.address_provider,
        vec![MarsContract::Oracle, MarsContract::ProtocolRewardsCollector],
    )?;
    let rewards_collector_addr = &addresses[&MarsContract::ProtocolRewardsCollector];
    let oracle_addr = &addresses[&MarsContract::Oracle];

    let mut response = Response::new();

    response =
        apply_accumulated_interests(&env, rewards_collector_addr, &mut borrow_market, response)?;

    // Set new debt
    let mut debt_amount_scaled =
        DEBTS.may_load(deps.storage, (&borrower_addr, &denom))?.unwrap_or_else(Uint128::zero);
    let borrow_amount_scaled =
        get_scaled_debt_amount(borrow_amount, &borrow_market, env.block.time.seconds())?;
    debt_amount_scaled = debt_amount_scaled.checked_add(borrow_amount_scaled)?;
    DEBTS.save(deps.storage, (&borrower_addr, &denom), &debt_amount_scaled)?;

    borrow_market.debt_total_scaled += borrow_amount_scaled;

    response =
        update_interest_rates(&deps, &env, &mut borrow_market, borrow_amount, &denom, response)?;
    MARKETS.save(deps.storage, &denom, &borrow_market)?;

    // once market and user debt has been updated, assert the user position is below max LTV
    if is_above_max_ltv(deps.as_ref(), &env, &borrower_addr, oracle_addr)? {
        return Err(ContractError::BorrowAmountExceedsGivenCollateral {});
    }

    // Send borrow amount to borrower or another recipient
    let recipient_addr = if let Some(recipient) = recipient {
        deps.api.addr_validate(&recipient)?
    } else {
        borrower_addr.clone()
    };

    Ok(response
        .add_message(build_send_asset_msg(&recipient_addr, &denom, borrow_amount))
        .add_attribute("action", "outposts/red-bank/borrow")
        .add_attribute("denom", denom)
        .add_attribute("user", borrower_addr)
        .add_attribute("recipient", recipient_addr)
        .add_attribute("amount", borrow_amount))
}

/// Handle the repay of native tokens. Refund extra funds if they exist
pub fn repay(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    on_behalf_of: Option<String>,
    denom: String,
    repay_amount: Uint128,
) -> Result<Response, ContractError> {
    let user_addr = if let Some(address) = on_behalf_of {
        deps.api.addr_validate(&address)?
    } else {
        info.sender.clone()
    };

    // Check new debt
    let debt_amount_scaled_before = DEBTS
        .may_load(deps.storage, (&user_addr, &denom))?
        .ok_or(ContractError::CannotRepayZeroDebt {})?;

    let config = CONFIG.load(deps.storage)?;

    let rewards_collector_addr = address_provider::helpers::query_address(
        deps.as_ref(),
        &config.address_provider,
        MarsContract::ProtocolRewardsCollector,
    )?;

    let mut market = MARKETS.load(deps.storage, &denom)?;

    let mut response = Response::new();

    response = apply_accumulated_interests(&env, &rewards_collector_addr, &mut market, response)?;

    let debt_amount_before =
        get_underlying_debt_amount(debt_amount_scaled_before, &market, env.block.time.seconds())?;

    // If repay amount exceeds debt, refund any excess amounts
    let mut refund_amount = Uint128::zero();
    let mut debt_amount_after = Uint128::zero();
    if repay_amount > debt_amount_before {
        refund_amount = repay_amount - debt_amount_before;
        let refund_msg = build_send_asset_msg(&user_addr, &denom, refund_amount);
        response = response.add_message(refund_msg);
    } else {
        debt_amount_after = debt_amount_before - repay_amount;
    }

    let debt_amount_scaled_after =
        get_scaled_debt_amount(debt_amount_after, &market, env.block.time.seconds())?;

    let debt_amount_scaled_delta =
        debt_amount_scaled_before.checked_sub(debt_amount_scaled_after)?;

    market.debt_total_scaled = market.debt_total_scaled.checked_sub(debt_amount_scaled_delta)?;

    response = update_interest_rates(&deps, &env, &mut market, Uint128::zero(), &denom, response)?;
    MARKETS.save(deps.storage, &denom, &market)?;

    // TODO: this logic can be extracted to a helper function to simplify the content of `excute.rs`
    if debt_amount_scaled_after.is_zero() {
        DEBTS.remove(deps.storage, (&user_addr, &denom));
    } else {
        DEBTS.save(deps.storage, (&user_addr, &denom), &debt_amount_scaled_after)?;
    }

    Ok(response
        .add_attribute("action", "outposts/red-bank/repay")
        .add_attribute("denom", denom)
        .add_attribute("sender", info.sender)
        .add_attribute("user", user_addr)
        .add_attribute("amount", repay_amount.checked_sub(refund_amount)?))
}

/// Execute loan liquidations on under-collateralized loans
pub fn liquidate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    collateral_denom: String,
    debt_denom: String,
    user_addr: Addr,
    sent_debt_asset_amount: Uint128,
) -> Result<Response, ContractError> {
    let block_time = env.block.time.seconds();

    // 1. Validate liquidation
    // check if the user has enabled the collateral asset as collateral
    let collateral = COLLATERALS
        .may_load(deps.storage, (&user_addr, &collateral_denom))?
        .ok_or(ContractError::CannotLiquidateWhenNoCollateralBalance {})?;
    if !collateral.enabled {
        return Err(ContractError::CannotLiquidateWhenCollateralUnset {
            denom: collateral_denom,
        });
    }

    // check if user has available collateral in specified collateral asset to be liquidated
    let collateral_market = MARKETS.load(deps.storage, &collateral_denom)?;
    let user_collateral_balance_scaled = cw20_get_balance(
        &deps.querier,
        collateral_market.ma_token_address.clone(),
        user_addr.clone(),
    )?;
    let user_collateral_balance = get_underlying_liquidity_amount(
        user_collateral_balance_scaled,
        &collateral_market,
        block_time,
    )?;
    if user_collateral_balance.is_zero() {
        return Err(ContractError::CannotLiquidateWhenNoCollateralBalance {});
    }

    // check if user has outstanding debt in the deposited asset that needs to be repayed
    let user_debt_amount_scaled = DEBTS
        .may_load(deps.storage, (&user_addr, &debt_denom))?
        .ok_or(ContractError::CannotLiquidateWhenNoDebtBalance {})?;

    // 2. Compute health factor
    let config = CONFIG.load(deps.storage)?;

    let addresses = address_provider::helpers::query_addresses(
        deps.as_ref(),
        &config.address_provider,
        vec![MarsContract::Oracle, MarsContract::ProtocolRewardsCollector],
    )?;
    let rewards_collector_addr = &addresses[&MarsContract::ProtocolRewardsCollector];
    let oracle_addr = &addresses[&MarsContract::Oracle];

    let (positions, health) =
        get_user_positions_health(deps.as_ref(), &env, &user_addr, oracle_addr)?;

    if !health.is_liquidatable() {
        return Err(ContractError::CannotLiquidateHealthyPosition {});
    }

    let collateral_and_debt_are_the_same_asset = debt_denom == collateral_denom;

    let debt_market = if !collateral_and_debt_are_the_same_asset {
        MARKETS.load(deps.storage, &debt_denom)?
    } else {
        collateral_market.clone()
    };

    // 3. Compute debt to repay and collateral to liquidate
    let collateral_price = positions
        .get(&collateral_denom)
        .ok_or(ContractError::CannotLiquidateWhenNoCollateralBalance {})?
        .asset_price;
    let debt_price = positions
        .get(&debt_denom)
        .ok_or(ContractError::CannotLiquidateWhenNoDebtBalance {})?
        .asset_price;

    let mut response = Response::new();

    let user_debt_asset_total_debt =
        get_underlying_debt_amount(user_debt_amount_scaled, &debt_market, block_time)?;

    let (debt_amount_to_repay, collateral_amount_to_liquidate, refund_amount) =
        liquidation_compute_amounts(
            collateral_price,
            debt_price,
            config.close_factor,
            user_collateral_balance,
            collateral_market.liquidation_bonus,
            user_debt_asset_total_debt,
            sent_debt_asset_amount,
        )?;

    // 4. Update collateral positions and market
    let collateral_amount_to_liquidate_scaled = get_scaled_liquidity_amount(
        collateral_amount_to_liquidate,
        &collateral_market,
        block_time,
    )?;

    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: collateral_market.ma_token_address.to_string(),
        msg: to_binary(&mars_outpost::ma_token::msg::ExecuteMsg::TransferOnLiquidation {
            sender: user_addr.to_string(),
            recipient: info.sender.to_string(),
            amount: collateral_amount_to_liquidate_scaled,
        })?,
        funds: vec![],
    }));

    // if max collateral to liquidate equals the user's balance, delete the collateral position
    if collateral_amount_to_liquidate_scaled == user_collateral_balance_scaled {
        COLLATERALS.remove(deps.storage, (&user_addr, &collateral_denom));
    }

    // 5. Compute and update user new debt
    let user_debt_asset_debt_amount_after =
        user_debt_asset_total_debt.checked_sub(debt_amount_to_repay)?;
    let user_debt_asset_debt_amount_scaled_after = get_scaled_debt_amount(
        user_debt_asset_debt_amount_after,
        &debt_market,
        env.block.time.seconds(),
    )?;

    // Compute delta so it can be substracted to total debt
    let debt_amount_scaled_delta =
        user_debt_amount_scaled.checked_sub(user_debt_asset_debt_amount_scaled_after)?;

    DEBTS.save(
        deps.storage,
        (&user_addr, &debt_denom),
        &user_debt_asset_debt_amount_scaled_after,
    )?;

    let debt_market_debt_total_scaled_after =
        debt_market.debt_total_scaled.checked_sub(debt_amount_scaled_delta)?;

    // 6. Update markets depending on whether the collateral and debt markets are the same
    // and whether the liquidator receives ma_tokens (no change in liquidity) or underlying asset
    // (changes liquidity)
    if collateral_and_debt_are_the_same_asset {
        // NOTE: for the sake of clarity copy attributes from collateral market and
        // give generic naming. Debt market could have been used as well
        let mut asset_market_after = collateral_market;
        let denom = &collateral_denom;

        response = apply_accumulated_interests(
            &env,
            rewards_collector_addr,
            &mut asset_market_after,
            response,
        )?;

        asset_market_after.debt_total_scaled = debt_market_debt_total_scaled_after;

        response = update_interest_rates(
            &deps,
            &env,
            &mut asset_market_after,
            refund_amount,
            denom,
            response,
        )?;

        MARKETS.save(deps.storage, denom, &asset_market_after)?;
    } else {
        let mut debt_market_after = debt_market;

        response = apply_accumulated_interests(
            &env,
            rewards_collector_addr,
            &mut debt_market_after,
            response,
        )?;

        debt_market_after.debt_total_scaled = debt_market_debt_total_scaled_after;

        response = update_interest_rates(
            &deps,
            &env,
            &mut debt_market_after,
            refund_amount,
            &debt_denom,
            response,
        )?;

        MARKETS.save(deps.storage, &debt_denom, &debt_market_after)?;
    }

    // 7. Build response
    // refund sent amount in excess of actual debt amount to liquidate
    if !refund_amount.is_zero() {
        response =
            response.add_message(build_send_asset_msg(&info.sender, &debt_denom, refund_amount));
    }

    Ok(response
        .add_attribute("action", "outposts/red-bank/liquidate")
        .add_attribute("collateral_denom", collateral_denom)
        .add_attribute("debt_denom", debt_denom)
        .add_attribute("user", user_addr.as_str())
        .add_attribute("liquidator", info.sender)
        .add_attribute("collateral_amount_liquidated", collateral_amount_to_liquidate.to_string())
        .add_attribute("debt_amount_repaid", debt_amount_to_repay.to_string())
        .add_attribute("refund_amount", refund_amount.to_string()))
}

/// Computes debt to repay (in debt asset),
/// collateral to liquidate (in collateral asset) and
/// amount to refund the liquidator (in debt asset)
fn liquidation_compute_amounts(
    collateral_price: Decimal,
    debt_price: Decimal,
    close_factor: Decimal,
    user_collateral_balance: Uint128,
    liquidation_bonus: Decimal,
    user_debt_asset_total_debt: Uint128,
    sent_debt_asset_amount: Uint128,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    // Debt: Only up to a fraction of the total debt (determined by the close factor) can be
    // repayed.
    let max_repayable_debt = close_factor * user_debt_asset_total_debt;

    let mut debt_amount_to_repay = if sent_debt_asset_amount > max_repayable_debt {
        max_repayable_debt
    } else {
        sent_debt_asset_amount
    };

    // Collateral: debt to repay in base asset times the liquidation
    // bonus
    let debt_amount_to_repay_in_base_asset = debt_amount_to_repay * debt_price;
    let collateral_amount_to_liquidate_in_base_asset =
        debt_amount_to_repay_in_base_asset * (Decimal::one() + liquidation_bonus);
    let mut collateral_amount_to_liquidate = math::divide_uint128_by_decimal(
        collateral_amount_to_liquidate_in_base_asset,
        collateral_price,
    )?;

    // If collateral amount to liquidate is higher than user_collateral_balance,
    // liquidate the full balance and adjust the debt amount to repay accordingly
    if collateral_amount_to_liquidate > user_collateral_balance {
        collateral_amount_to_liquidate = user_collateral_balance;
        debt_amount_to_repay = math::divide_uint128_by_decimal(
            math::divide_uint128_by_decimal(
                collateral_amount_to_liquidate * collateral_price,
                debt_price,
            )?,
            Decimal::one() + liquidation_bonus,
        )?
    }

    let refund_amount = sent_debt_asset_amount - debt_amount_to_repay;

    Ok((debt_amount_to_repay, collateral_amount_to_liquidate, refund_amount))
}

/// Update (enable / disable) collateral asset for specific user
pub fn update_asset_collateral_status(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    denom: String,
    enable: bool,
) -> Result<Response, ContractError> {
    let user_addr = info.sender;
    let mut collateral =
        COLLATERALS.may_load(deps.storage, (&user_addr, &denom))?.unwrap_or_default();

    let collateral_market = MARKETS.load(deps.storage, &denom)?;

    if !collateral.enabled && enable {
        let collateral_ma_address = collateral_market.ma_token_address;
        let user_collateral_balance =
            cw20_get_balance(&deps.querier, collateral_ma_address, user_addr.clone())?;
        if !user_collateral_balance.is_zero() {
            // enable collateral asset
            collateral.enabled = true;
            COLLATERALS.save(deps.storage, (&user_addr, &denom), &collateral)?;
        } else {
            return Err(ContractError::UserNoCollateralBalance {
                user: user_addr.to_string(),
                denom,
            });
        }
    } else if collateral.enabled && !enable {
        // disable collateral asset
        collateral.enabled = false;
        COLLATERALS.save(deps.storage, (&user_addr, &denom), &collateral)?;

        // check health factor after disabling collateral
        let config = CONFIG.load(deps.storage)?;
        let oracle_addr = address_provider::helpers::query_address(
            deps.as_ref(),
            &config.address_provider,
            MarsContract::Oracle,
        )?;

        if is_liquidatable(deps.as_ref(), &env, &user_addr, &oracle_addr)? {
            return Err(ContractError::InvalidHealthFactorAfterDisablingCollateral {});
        }
    }

    Ok(Response::new()
        .add_attribute("action", "outposts/red-bank/update_asset_collateral_status")
        .add_attribute("user", user_addr.as_str())
        .add_attribute("denom", denom)
        .add_attribute("enable", enable.to_string()))
}

/// Update uncollateralized loan limit by a given amount in base asset
pub fn finalize_liquidity_token_transfer(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    from_address: Addr,
    to_address: Addr,
    from_previous_balance: Uint128,
    to_previous_balance: Uint128,
    amount: Uint128,
) -> Result<Response, ContractError> {
    // Get liquidity token market
    let denom = MARKET_DENOMS_BY_MA_TOKEN.load(deps.storage, &info.sender)?;

    // Check user health factor is above 1
    let config = CONFIG.load(deps.storage)?;
    let oracle_addr = address_provider::helpers::query_address(
        deps.as_ref(),
        &config.address_provider,
        MarsContract::Oracle,
    )?;

    if is_liquidatable(deps.as_ref(), &env, &from_address, &oracle_addr)? {
        return Err(ContractError::CannotTransferTokenWhenInvalidHealthFactor {});
    }

    // Update users's positions
    if from_address != to_address {
        if from_previous_balance.checked_sub(amount)?.is_zero() {
            COLLATERALS.remove(deps.storage, (&from_address, &denom));
        }

        if to_previous_balance.is_zero() && !amount.is_zero() {
            COLLATERALS.save(
                deps.storage,
                (&to_address, &denom),
                &Collateral {
                    enabled: true,
                },
            )?;
        }
    }

    Ok(Response::new()
        .add_attribute("action", "outposts/red-bank/finalize_liquidity_token_transfer"))
}
