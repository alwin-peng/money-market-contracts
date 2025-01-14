use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{CanonicalAddr, Deps, Order, StdResult, Storage};
use cosmwasm_storage::{Bucket, ReadonlyBucket, ReadonlySingleton, Singleton};
use moneymarket::custody::{BAssetInfo, BorrowerResponse};

const KEY_CONFIG: &[u8] = b"config";
const PREFIX_BORROWER: &[u8] = b"borrower";
const KEY_TOTAL_CUMULATIVE_REWARDS: &[u8] = b"total_cumulative_rewards";
const KEY_CURRENT_REBASE_INDEX: &[u8] = b"current_rebase_index";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub oracle: CanonicalAddr,
    pub collateral_token: CanonicalAddr,
    pub underlying_token: CanonicalAddr,
    pub overseer_contract: CanonicalAddr,
    pub market_contract: CanonicalAddr,
    pub liquidation_contract: CanonicalAddr,
    pub stable_denom: String,
    pub basset_info: BAssetInfo,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BorrowerInfo {
    pub balance: Uint256,
    pub spendable: Uint256,
}

pub fn store_config(storage: &mut dyn Storage, data: &Config) -> StdResult<()> {
    Singleton::new(storage, KEY_CONFIG).save(data)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    ReadonlySingleton::new(storage, KEY_CONFIG).load()
}

pub fn store_borrower_info(
    storage: &mut dyn Storage,
    borrower: &CanonicalAddr,
    borrower_info: &BorrowerInfo,
) -> StdResult<()> {
    let mut borrower_bucket: Bucket<BorrowerInfo> = Bucket::new(storage, PREFIX_BORROWER);
    borrower_bucket.save(borrower.as_slice(), borrower_info)?;

    Ok(())
}

pub fn remove_borrower_info(storage: &mut dyn Storage, borrower: &CanonicalAddr) {
    let mut borrower_bucket: Bucket<BorrowerInfo> = Bucket::new(storage, PREFIX_BORROWER);
    borrower_bucket.remove(borrower.as_slice());
}

pub fn read_borrower_info(storage: &dyn Storage, borrower: &CanonicalAddr) -> BorrowerInfo {
    let borrower_bucket: ReadonlyBucket<BorrowerInfo> =
        ReadonlyBucket::new(storage, PREFIX_BORROWER);
    match borrower_bucket.load(borrower.as_slice()) {
        Ok(v) => v,
        _ => BorrowerInfo {
            balance: Uint256::zero(),
            spendable: Uint256::zero(),
        },
    }
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
pub fn read_borrowers(
    deps: Deps,
    start_after: Option<CanonicalAddr>,
    limit: Option<u32>,
) -> StdResult<Vec<BorrowerResponse>> {
    let position_bucket: ReadonlyBucket<BorrowerInfo> =
        ReadonlyBucket::new(deps.storage, PREFIX_BORROWER);

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = calc_range_start(start_after);

    position_bucket
        .range(start.as_deref(), None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (k, v) = item?;
            let borrower: CanonicalAddr = CanonicalAddr::from(k);
            Ok(BorrowerResponse {
                borrower: deps.api.addr_humanize(&borrower)?.to_string(),
                balance: v.balance,
                spendable: v.spendable,
            })
        })
        .collect()
}

// this will set the first key after the provided key, by appending a 1 byte
fn calc_range_start(start_after: Option<CanonicalAddr>) -> Option<Vec<u8>> {
    start_after.map(|addr| {
        let mut v = addr.as_slice().to_vec();
        v.push(1);
        v
    })
}

pub fn save_current_rebase_index(
    storage: &mut dyn Storage,
    new_rebase_index: &Decimal256,
) -> StdResult<()> {
    let mut rebase_index = Singleton::new(storage, KEY_CURRENT_REBASE_INDEX);
    rebase_index.save(new_rebase_index)
}

pub fn read_current_rebase_index(storage: &mut dyn Storage) -> StdResult<Decimal256> {
    ReadonlySingleton::new(storage, KEY_CURRENT_REBASE_INDEX).load()
}

pub fn save_total_cumulative_rewards(
    storage: &mut dyn Storage,
    new_rewards: &Uint256,
) -> StdResult<()> {
    let mut rewards = Singleton::new(storage, KEY_TOTAL_CUMULATIVE_REWARDS);
    rewards.save(new_rewards)
}

pub fn read_total_cumulative_rewards(storage: &dyn Storage) -> Uint256 {
    ReadonlySingleton::new(storage, KEY_TOTAL_CUMULATIVE_REWARDS)
        .load()
        .unwrap_or_else(|_| Uint256::zero())
}

pub fn update_total_cumulative_rewards(storage: &mut dyn Storage, data: &Uint256) -> StdResult<()> {
    let mut current = read_total_cumulative_rewards(storage);
    current += *data;
    save_total_cumulative_rewards(storage, &current)
}
