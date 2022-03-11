use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap};
use near_sdk::json_types::{U128, U64, ValidAccountId};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, near_bindgen, AccountId, Balance, BorshStorageKey,
    PanicOnDefault, PromiseOrValue, Gas, testing_env
};
use std::collections::HashMap;
use near_contract_standards::fungible_token::core_impl::ext_fungible_token;
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;

near_sdk::setup_alloc!();

pub use crate::proposals::*;
pub use crate::types::*;
pub use crate::views::*;
pub use crate::types::*;
pub use crate::donations::*;
pub use crate::bounty::*;
use crate::utils::*;

mod delegation;
mod proposals;
mod types;
pub mod views;
mod donations;
mod bounty;
mod utils;

#[derive(BorshStorageKey, BorshSerialize)]
pub enum StorageKeys {
    Delegations,
    Proposals,
    Donations,
    Bounties
}

#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize, PanicOnDefault)]
pub struct Contract {
    // DAO Metadata.
    pub dao_metadata: DaoMetadata,
    // Voting and permissions policy.

    // Amount of $NEAR locked for bonds.
    pub locked_amount: Balance,

    // Vote staking contract id. That contract must have this account as owner.
    pub token_account_id: OldAccountId,
    // Delegated  token total amount.
    pub total_delegation_amount: Balance,
    // Delegations per user.
    pub delegations: LookupMap<AccountId, Balance>,
    // Last available id for the proposals.
    pub last_proposal_id: u64,
    // Proposal map from ID to proposal information.
    pub proposals: LookupMap<u64, VersionedProposal>,

    pub donations: LookupMap<AccountId, Balance>,

    pub owner_id: AccountId,

    pub last_bounty_id: u64,

    pub bounties: LookupMap<u64, VersionedBounty>,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(metadata: DaoMetadata, token_contract_id: AccountId) -> Self {
        let owner_id = env::predecessor_account_id();
        let this = Self {
            dao_metadata: metadata,
            token_account_id: token_contract_id,
            total_delegation_amount: 0,
            delegations: LookupMap::new(StorageKeys::Delegations),
            last_proposal_id: 0,
            proposals: LookupMap::new(StorageKeys::Proposals),
            locked_amount: 0,
            donations: LookupMap::new(StorageKeys::Donations),
            owner_id: owner_id,
            last_bounty_id: 0,
            bounties: LookupMap::new(StorageKeys::Bounties),
        };
        this
    }

    // Should only be called by this contract on migration.
    // This is NOOP implementation. KEEP IT if you haven't changed contract state.
    // If you have changed state, you need to implement migration from old state (keep the old struct with different name to deserialize it first).
    // After migrate goes live on MainNet, return this implementation for next updates.
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "ERR_NOT_ALLOWED"
        );
        let this: Contract = env::state_read().expect("ERR_CONTRACT_IS_NOT_INITIALIZED");
        this
    }
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Clone, Debug))]
#[serde(crate = "near_sdk::serde")]
pub enum TransferPurpose {
    Delegate,
    OpenDonate,
    ProposalDonate,
    CreateBounty,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Clone, Debug))]
#[serde(crate = "near_sdk::serde")]
pub struct TransferArgs {
    pub delegate: AccountId,
    pub proposal: Option<u64>,
    pub transfer_type: TransferPurpose, // 1 for delegate, 2 for open donate, 3 for proposal donate,
    pub bounty_input: Option<BountyInput>
}

/**
    Delegate for user
    User transfer token to dao contract
    in ft_on_transfer function
    The delegation of user will be added the amount of token transferred  to contract, delegate is the delegate property of DelegateArgs
*/
// #[near_bindgen]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: ValidAccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let TransferArgs { delegate, proposal, transfer_type, bounty_input } = near_sdk::serde_json::from_str(&msg).expect("Not valid DelegateArgs");
        let token_account_id = self.token_account_id.clone();
        match transfer_type {
            TransferPurpose::Delegate => {
                assert_account_id(&token_account_id);
                self.internal_delegate(&delegate, amount);
                self.locked_amount += amount.0;
            },
            TransferPurpose::OpenDonate => {
                assert_account_id(&token_account_id);
                self.open_donate(&sender_id.to_string(), amount);
            }, 
            TransferPurpose::ProposalDonate => {
                assert_account_id(&token_account_id);
                let proposal_id = proposal.expect("PROPOSAL_ID_NOT_PROVIDED");
                let mut proposal_obj: Proposal = self.proposals.get(&proposal_id).expect("ERR_NO_PROPOSAL").into();
                match proposal_obj.kind {
                    ProposalKind::Donate => {
                        proposal_obj.donate(&sender_id.to_string(), amount.0);
                    },
                    _ => {
                        assert!(
                            proposal_obj.kind.eq(&ProposalKind::Donate),
                            "PROPOSAL_IS_NOT_DONATION_KIND"
                        )
                    },
                    
                }
            },
            TransferPurpose::CreateBounty => {
                let bounty_unwrapped = bounty_input.expect("BOUNTY_INPUT_NOT_FOUND");
                assert_account_id(&bounty_unwrapped.token);
                self.create_bounty(bounty_unwrapped);
            }
        }
        PromiseOrValue::Value(U128(0))
    }
}
