use crate::borrow::{compute_interest, compute_loan};
use crate::state::{store_state, Config, Liability, State};
use crate::testing::mock_querier::mock_dependencies;
use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::testing::{mock_env, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{Api, Coin, HumanAddr, Uint128};

#[test]
fn proper_compute_loan() {
    let env = mock_env("addr0000", &[]);
    let mock_state = State {
        total_liabilities: Decimal256::from_uint256(1000000u128),
        total_reserves: Decimal256::from_uint256(0u128),
        last_interest_updated: env.block.height,
        global_interest_index: Decimal256::one(),
    };
    let mut liability1 = Liability {
        interest_index: Decimal256::one(),
        loan_amount: Uint256::zero(),
    };
    compute_loan(&mock_state, &mut liability1);
    let liability2 = Liability {
        interest_index: Decimal256::one(),
        loan_amount: Uint256::zero(),
    };
    assert_eq!(liability1, liability2);

    let mock_state2 = State {
        total_liabilities: Decimal256::from_uint256(300000u128),
        total_reserves: Decimal256::from_uint256(1000u128),
        last_interest_updated: env.block.height,
        global_interest_index: Decimal256::from_uint256(2u128),
    };
    let mut liability3 = Liability {
        interest_index: Decimal256::from_uint256(4u128),
        loan_amount: Uint256::from(80u128),
    };
    compute_loan(&mock_state2, &mut liability3);
    let liability4 = Liability {
        interest_index: Decimal256::from_uint256(2u128),
        loan_amount: Uint256::from(40u128),
    };
    assert_eq!(liability3, liability4);
}

#[test]
fn proper_compute_interest() {
    let mut deps = mock_dependencies(
        20,
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128(2000000u128),
        }],
    );

    let mut env = mock_env("addr0000", &[]);

    let mock_config = Config {
        contract_addr: deps
            .api
            .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
            .unwrap(),
        owner_addr: deps
            .api
            .canonical_address(&HumanAddr::from("owner"))
            .unwrap(),
        anchor_token: deps
            .api
            .canonical_address(&HumanAddr::from("AT-uusd"))
            .unwrap(),
        interest_model: deps
            .api
            .canonical_address(&HumanAddr::from("interest"))
            .unwrap(),
        overseer_contract: deps
            .api
            .canonical_address(&HumanAddr::from("overseer"))
            .unwrap(),
        stable_denom: "uusd".to_string(),
        reserve_factor: Decimal256::permille(3),
    };

    deps.querier
        .with_borrow_rate(&[(&HumanAddr::from("interest"), &Decimal256::percent(1))]);

    let mut mock_state = State {
        total_liabilities: Decimal256::from_uint256(1000000u128),
        total_reserves: Decimal256::zero(),
        last_interest_updated: env.block.height,
        global_interest_index: Decimal256::one(),
    };
    store_state(&mut deps.storage, &mock_state).unwrap();

    let mock_deposit_amount = Some(Uint256::from(1000u128));

    compute_interest(
        &deps,
        &mock_config,
        &mut mock_state,
        env.block.height,
        mock_deposit_amount,
    )
    .unwrap();
    assert_eq!(
        mock_state,
        State {
            global_interest_index: Decimal256::from_uint256(1u128),
            total_liabilities: Decimal256::from_uint256(1000000u128),
            total_reserves: Decimal256::zero(),
            last_interest_updated: env.block.height,
        }
    );

    env.block.height += 100;

    compute_interest(
        &deps,
        &mock_config,
        &mut mock_state,
        env.block.height,
        mock_deposit_amount,
    )
    .unwrap();
    assert_eq!(
        mock_state,
        State {
            global_interest_index: Decimal256::from_uint256(2u128),
            total_liabilities: Decimal256::from_uint256(2000000u128),
            total_reserves: Decimal256::from_uint256(3000u128),
            last_interest_updated: env.block.height,
        }
    );
}