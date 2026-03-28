#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
pub enum DataKey {
    Owner,
    Admin(Address),
    ProposalCounter,
    Proposal(u64),
    HasVoted(u64, Address),
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalType {
    AddAdmin(Address),
    RemoveAdmin(Address),
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProposalStatus {
    Pending = 0,
    Executed = 1,
    Rejected = 2,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub creator: Address,
    pub prop_type: ProposalType,
    pub status: ProposalStatus,
    pub votes_for: u32,
    pub votes_against: u32,
    pub created_at: u64,
}

// ---------------------------------------------------------------------------
// Event data structures
//
// Each struct is published as the *data* portion of an event so that
// off-chain indexers can deserialise a single, self-contained value rather
// than having to correlate multiple positional fields.
// ---------------------------------------------------------------------------

/// Emitted when a new governance proposal is created.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ProposalCreatedEvent {
    /// Unique proposal identifier.
    pub proposal_id: u64,
    /// Address that created the proposal.
    pub proposer: Address,
    /// Ledger timestamp at creation time.
    pub timestamp: u64,
}

/// Emitted when an admin casts a vote on a proposal.
#[contracttype]
#[derive(Clone, Debug)]
pub struct VoteCastEvent {
    /// Proposal being voted on.
    pub proposal_id: u64,
    /// Address of the voter.
    pub voter: Address,
    /// `true` = vote in favour, `false` = vote against.
    pub support: bool,
    /// Voting power applied (currently always 1 per admin).
    pub voting_power: u32,
    /// Ledger timestamp of the vote.
    pub timestamp: u64,
}

/// Emitted when a proposal is executed or rejected.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ProposalExecutedEvent {
    /// Proposal that was finalised.
    pub proposal_id: u64,
    /// Address that triggered execution.
    pub executor: Address,
    /// Final status: 1 = Executed, 2 = Rejected.
    pub result: u32,
    /// Total votes in favour at execution time.
    pub votes_for: u32,
    /// Total votes against at execution time.
    pub votes_against: u32,
    /// Ledger timestamp of execution.
    pub timestamp: u64,
}

/// Emitted when an admin is added (directly or via proposal).
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminAddedEvent {
    /// The newly added admin address.
    pub admin: Address,
    /// Address that authorised the addition (owner or "proposal").
    pub added_by: Address,
    /// Ledger timestamp.
    pub timestamp: u64,
}

/// Emitted when an admin is removed (directly or via proposal).
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminRemovedEvent {
    /// The removed admin address.
    pub admin: Address,
    /// Address that authorised the removal (owner or "proposal").
    pub removed_by: Address,
    /// Ledger timestamp.
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Event topic symbols (max 9 chars for symbol_short!)
// ---------------------------------------------------------------------------

const GOVERNANCE: Symbol = symbol_short!("gov");

// Topic symbols – kept short to stay within Soroban's 9-char limit.
const EVT_PROP_NEW: Symbol = symbol_short!("prop_new");
const EVT_VOTE: Symbol = symbol_short!("voted");
const EVT_PROP_EXEC: Symbol = symbol_short!("prop_exec");
const EVT_ADM_ADD: Symbol = symbol_short!("adm_add");
const EVT_ADM_REM: Symbol = symbol_short!("adm_rem");

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the governance contract with an owner.
    pub fn init(env: Env, owner: Address) -> bool {
        owner.require_auth();
        env.storage().persistent().set(&DataKey::Owner, &owner);
        true
    }

    // -----------------------------------------------------------------------
    // Admin management
    // -----------------------------------------------------------------------

    /// Add an admin. Only the owner may call this.
    pub fn add_admin(env: Env, owner: Address, admin: Address) -> bool {
        owner.require_auth();
        let stored_owner: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Owner)
            .expect("Governance not initialized");
        if stored_owner != owner {
            panic!("Only owner can add admins");
        }
        env.storage()
            .persistent()
            .set(&DataKey::Admin(admin.clone()), &true);

        env.events().publish(
            (GOVERNANCE, EVT_ADM_ADD),
            AdminAddedEvent {
                admin,
                added_by: owner,
                timestamp: env.ledger().timestamp(),
            },
        );
        true
    }

    /// Remove an admin. Only the owner may call this.
    pub fn remove_admin(env: Env, owner: Address, admin: Address) -> bool {
        owner.require_auth();
        let stored_owner: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Owner)
            .expect("Governance not initialized");
        if stored_owner != owner {
            panic!("Only owner can remove admins");
        }
        env.storage()
            .persistent()
            .remove(&DataKey::Admin(admin.clone()));

        env.events().publish(
            (GOVERNANCE, EVT_ADM_REM),
            AdminRemovedEvent {
                admin,
                removed_by: owner,
                timestamp: env.ledger().timestamp(),
            },
        );
        true
    }

    /// Returns `true` if `addr` is a registered admin.
    pub fn is_admin(env: Env, addr: Address) -> bool {
        env.storage()
            .persistent()
            .get::<DataKey, bool>(&DataKey::Admin(addr))
            .unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Proposal lifecycle
    // -----------------------------------------------------------------------

    /// Create a new governance proposal. Only admins may propose.
    ///
    /// # Events
    /// Emits `(gov, prop_new) → ProposalCreatedEvent` on success.
    pub fn create_proposal(env: Env, creator: Address, prop_type: ProposalType) -> u64 {
        creator.require_auth();
        if !Self::is_admin(env.clone(), creator.clone()) {
            panic!("Only admins can create proposals");
        }

        let mut counter: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ProposalCounter)
            .unwrap_or(0);
        counter += 1;

        let now = env.ledger().timestamp();
        let proposal = Proposal {
            id: counter,
            creator: creator.clone(),
            prop_type,
            status: ProposalStatus::Pending,
            votes_for: 0,
            votes_against: 0,
            created_at: now,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Proposal(counter), &proposal);
        env.storage()
            .persistent()
            .set(&DataKey::ProposalCounter, &counter);

        env.events().publish(
            (GOVERNANCE, EVT_PROP_NEW),
            ProposalCreatedEvent {
                proposal_id: counter,
                proposer: creator,
                timestamp: now,
            },
        );

        counter
    }

    /// Cast a vote on a Pending proposal. Each admin may vote once.
    ///
    /// # Events
    /// Emits `(gov, voted) → VoteCastEvent` on success.
    pub fn vote(env: Env, voter: Address, proposal_id: u64, support: bool) -> bool {
        voter.require_auth();
        if !Self::is_admin(env.clone(), voter.clone()) {
            panic!("Only admins can vote");
        }

        let key = DataKey::Proposal(proposal_id);
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Proposal not found");

        if proposal.status != ProposalStatus::Pending {
            panic!("Proposal is not in Pending status");
        }

        let voted_key = DataKey::HasVoted(proposal_id, voter.clone());
        if env.storage().persistent().has(&voted_key) {
            panic!("Already voted");
        }

        if support {
            proposal.votes_for += 1;
        } else {
            proposal.votes_against += 1;
        }

        env.storage().persistent().set(&key, &proposal);
        env.storage().persistent().set(&voted_key, &true);

        env.events().publish(
            (GOVERNANCE, EVT_VOTE),
            VoteCastEvent {
                proposal_id,
                voter,
                support,
                voting_power: 1,
                timestamp: env.ledger().timestamp(),
            },
        );

        true
    }

    /// Execute or reject a Pending proposal. Only admins may trigger execution.
    ///
    /// A proposal is **executed** when `votes_for > votes_against && votes_for > 0`;
    /// otherwise it is **rejected**.
    ///
    /// # Events
    /// - Emits `(gov, prop_exec) → ProposalExecutedEvent` always.
    /// - Emits `(gov, adm_add) → AdminAddedEvent` when an AddAdmin proposal executes.
    /// - Emits `(gov, adm_rem) → AdminRemovedEvent` when a RemoveAdmin proposal executes.
    pub fn execute_proposal(env: Env, caller: Address, proposal_id: u64) -> bool {
        caller.require_auth();
        if !Self::is_admin(env.clone(), caller.clone()) {
            panic!("Only admins can execute proposals");
        }

        let key = DataKey::Proposal(proposal_id);
        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Proposal not found");

        if proposal.status != ProposalStatus::Pending {
            panic!("Proposal is not in Pending status");
        }

        let now = env.ledger().timestamp();

        if proposal.votes_for > proposal.votes_against && proposal.votes_for > 0 {
            proposal.status = ProposalStatus::Executed;

            match &proposal.prop_type {
                ProposalType::AddAdmin(new_admin) => {
                    env.storage()
                        .persistent()
                        .set(&DataKey::Admin(new_admin.clone()), &true);
                    env.events().publish(
                        (GOVERNANCE, EVT_ADM_ADD),
                        AdminAddedEvent {
                            admin: new_admin.clone(),
                            added_by: caller.clone(),
                            timestamp: now,
                        },
                    );
                }
                ProposalType::RemoveAdmin(old_admin) => {
                    env.storage()
                        .persistent()
                        .remove(&DataKey::Admin(old_admin.clone()));
                    env.events().publish(
                        (GOVERNANCE, EVT_ADM_REM),
                        AdminRemovedEvent {
                            admin: old_admin.clone(),
                            removed_by: caller.clone(),
                            timestamp: now,
                        },
                    );
                }
            }
        } else {
            proposal.status = ProposalStatus::Rejected;
        }

        let result_code = proposal.status as u32;
        let votes_for = proposal.votes_for;
        let votes_against = proposal.votes_against;

        env.storage().persistent().set(&key, &proposal);

        env.events().publish(
            (GOVERNANCE, EVT_PROP_EXEC),
            ProposalExecutedEvent {
                proposal_id,
                executor: caller,
                result: result_code,
                votes_for,
                votes_against,
                timestamp: now,
            },
        );

        true
    }

    /// Retrieve full proposal details by ID.
    pub fn get_proposal(env: Env, proposal_id: u64) -> Proposal {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .expect("Proposal not found")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Events, Ledger};
    use soroban_sdk::{Env, IntoVal};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    struct TestEnv {
        env: Env,
        owner: Address,
        contract_id: Address,
    }

    impl TestEnv {
        fn new() -> Self {
            let env = Env::default();
            env.mock_all_auths();
            let contract_id = env.register(GovernanceContract, ());
            let owner = Address::generate(&env);
            GovernanceContractClient::new(&env, &contract_id).init(&owner);
            TestEnv { env, owner, contract_id }
        }

        fn client(&self) -> GovernanceContractClient {
            GovernanceContractClient::new(&self.env, &self.contract_id)
        }
    }

    // -----------------------------------------------------------------------
    // Admin management – existing behaviour preserved
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_and_check_admin() {
        let t = TestEnv::new();
        let client = t.client();
        let admin = Address::generate(&t.env);
        assert!(client.add_admin(&t.owner, &admin));
        assert!(client.is_admin(&admin));
        assert!(client.remove_admin(&t.owner, &admin));
        assert!(!client.is_admin(&admin));
    }

    // -----------------------------------------------------------------------
    // Proposal lifecycle – existing behaviour preserved
    // -----------------------------------------------------------------------

    #[test]
    fn test_proposal_lifecycle() {
        let t = TestEnv::new();
        let client = t.client();
        let admin1 = Address::generate(&t.env);
        let admin2 = Address::generate(&t.env);
        let new_admin = Address::generate(&t.env);

        client.add_admin(&t.owner, &admin1);
        client.add_admin(&t.owner, &admin2);

        let prop_id = client.create_proposal(&admin1, &ProposalType::AddAdmin(new_admin.clone()));
        assert_eq!(prop_id, 1);
        assert_eq!(client.get_proposal(&prop_id).status, ProposalStatus::Pending);

        client.vote(&admin1, &prop_id, &true);
        assert_eq!(client.get_proposal(&prop_id).votes_for, 1);

        assert!(client.execute_proposal(&admin2, &prop_id));
        assert_eq!(client.get_proposal(&prop_id).status, ProposalStatus::Executed);
        assert!(client.is_admin(&new_admin));
    }

    #[test]
    #[should_panic(expected = "Already voted")]
    fn test_double_vote_panic() {
        let t = TestEnv::new();
        let client = t.client();
        let admin = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin);
        let prop_id = client.create_proposal(&admin, &ProposalType::AddAdmin(target));
        client.vote(&admin, &prop_id, &true);
        client.vote(&admin, &prop_id, &true);
    }

    #[test]
    #[should_panic(expected = "Only admins can create proposals")]
    fn test_unauthorized_propose_panic() {
        let t = TestEnv::new();
        let client = t.client();
        let rando = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        client.create_proposal(&rando, &ProposalType::AddAdmin(target));
    }

    // -----------------------------------------------------------------------
    // Event: ProposalCreatedEvent
    // -----------------------------------------------------------------------

    #[test]
    fn test_event_proposal_created_emitted() {
        let t = TestEnv::new();
        let client = t.client();
        let admin = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin);

        t.env.ledger().set_timestamp(42);
        let prop_id = client.create_proposal(&admin, &ProposalType::AddAdmin(target.clone()));

        let all_events = t.env.events().all();
        let prop_new_sym: Symbol = symbol_short!("prop_new");
        let gov_sym: Symbol = symbol_short!("gov");

        let found = all_events.iter().any(|e| {
            let topics = e.0;
            topics.len() >= 2
                && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                && topics.get(1) == Some(prop_new_sym.clone().into_val(&t.env))
        });
        assert!(found, "prop_new event not found");

        let prop_event = all_events
            .iter()
            .find(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(prop_new_sym.clone().into_val(&t.env))
            })
            .expect("prop_new event missing");

        let data: ProposalCreatedEvent = prop_event.1.into_val(&t.env);
        assert_eq!(data.proposal_id, prop_id);
        assert_eq!(data.proposer, admin);
        assert_eq!(data.timestamp, 42);
    }

    #[test]
    fn test_event_proposal_created_increments_id() {
        let t = TestEnv::new();
        let client = t.client();
        let admin = Address::generate(&t.env);
        let t1 = Address::generate(&t.env);
        let t2 = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin);

        let id1 = client.create_proposal(&admin, &ProposalType::AddAdmin(t1));
        let id2 = client.create_proposal(&admin, &ProposalType::AddAdmin(t2));
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);

        let gov_sym: Symbol = symbol_short!("gov");
        let prop_new_sym: Symbol = symbol_short!("prop_new");
        let count = t.env
            .events()
            .all()
            .iter()
            .filter(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(prop_new_sym.clone().into_val(&t.env))
            })
            .count();
        assert_eq!(count, 2);
    }

    // -----------------------------------------------------------------------
    // Event: VoteCastEvent
    // -----------------------------------------------------------------------

    #[test]
    fn test_event_vote_cast_for() {
        let t = TestEnv::new();
        let client = t.client();
        let admin = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin);
        let prop_id = client.create_proposal(&admin, &ProposalType::AddAdmin(target));

        t.env.ledger().set_timestamp(100);
        client.vote(&admin, &prop_id, &true);

        let gov_sym: Symbol = symbol_short!("gov");
        let voted_sym: Symbol = symbol_short!("voted");

        let vote_event = t.env
            .events()
            .all()
            .iter()
            .find(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(voted_sym.clone().into_val(&t.env))
            })
            .expect("voted event missing");

        let data: VoteCastEvent = vote_event.1.into_val(&t.env);
        assert_eq!(data.proposal_id, prop_id);
        assert_eq!(data.voter, admin);
        assert!(data.support);
        assert_eq!(data.voting_power, 1);
        assert_eq!(data.timestamp, 100);
    }

    #[test]
    fn test_event_vote_cast_against() {
        let t = TestEnv::new();
        let client = t.client();
        let admin = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin);
        let prop_id = client.create_proposal(&admin, &ProposalType::AddAdmin(target));

        client.vote(&admin, &prop_id, &false);

        let gov_sym: Symbol = symbol_short!("gov");
        let voted_sym: Symbol = symbol_short!("voted");

        let vote_event = t.env
            .events()
            .all()
            .iter()
            .find(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(voted_sym.clone().into_val(&t.env))
            })
            .expect("voted event missing");

        let data: VoteCastEvent = vote_event.1.into_val(&t.env);
        assert!(!data.support);
        assert_eq!(data.voting_power, 1);
    }

    #[test]
    fn test_event_multiple_votes_emitted() {
        let t = TestEnv::new();
        let client = t.client();
        let admin1 = Address::generate(&t.env);
        let admin2 = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin1);
        client.add_admin(&t.owner, &admin2);
        let prop_id = client.create_proposal(&admin1, &ProposalType::AddAdmin(target));

        client.vote(&admin1, &prop_id, &true);
        client.vote(&admin2, &prop_id, &false);

        let gov_sym: Symbol = symbol_short!("gov");
        let voted_sym: Symbol = symbol_short!("voted");

        let count = t.env
            .events()
            .all()
            .iter()
            .filter(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(voted_sym.clone().into_val(&t.env))
            })
            .count();
        assert_eq!(count, 2);
    }

    // -----------------------------------------------------------------------
    // Event: ProposalExecutedEvent – executed path
    // -----------------------------------------------------------------------

    #[test]
    fn test_event_proposal_executed_success() {
        let t = TestEnv::new();
        let client = t.client();
        let admin1 = Address::generate(&t.env);
        let admin2 = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin1);
        client.add_admin(&t.owner, &admin2);
        let prop_id = client.create_proposal(&admin1, &ProposalType::AddAdmin(target));
        client.vote(&admin1, &prop_id, &true);

        t.env.ledger().set_timestamp(200);
        client.execute_proposal(&admin2, &prop_id);

        let gov_sym: Symbol = symbol_short!("gov");
        let exec_sym: Symbol = symbol_short!("prop_exec");

        let exec_event = t.env
            .events()
            .all()
            .iter()
            .find(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(exec_sym.clone().into_val(&t.env))
            })
            .expect("prop_exec event missing");

        let data: ProposalExecutedEvent = exec_event.1.into_val(&t.env);
        assert_eq!(data.proposal_id, prop_id);
        assert_eq!(data.executor, admin2);
        assert_eq!(data.result, ProposalStatus::Executed as u32);
        assert_eq!(data.votes_for, 1);
        assert_eq!(data.votes_against, 0);
        assert_eq!(data.timestamp, 200);
    }

    // -----------------------------------------------------------------------
    // Event: ProposalExecutedEvent – rejected path
    // -----------------------------------------------------------------------

    #[test]
    fn test_event_proposal_rejected() {
        let t = TestEnv::new();
        let client = t.client();
        let admin1 = Address::generate(&t.env);
        let admin2 = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin1);
        client.add_admin(&t.owner, &admin2);
        let prop_id = client.create_proposal(&admin1, &ProposalType::AddAdmin(target));
        client.vote(&admin1, &prop_id, &false);
        client.execute_proposal(&admin2, &prop_id);

        let gov_sym: Symbol = symbol_short!("gov");
        let exec_sym: Symbol = symbol_short!("prop_exec");

        let exec_event = t.env
            .events()
            .all()
            .iter()
            .find(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(exec_sym.clone().into_val(&t.env))
            })
            .expect("prop_exec event missing");

        let data: ProposalExecutedEvent = exec_event.1.into_val(&t.env);
        assert_eq!(data.result, ProposalStatus::Rejected as u32);
        assert_eq!(data.votes_for, 0);
        assert_eq!(data.votes_against, 1);
    }

    #[test]
    fn test_event_proposal_rejected_no_votes() {
        let t = TestEnv::new();
        let client = t.client();
        let admin1 = Address::generate(&t.env);
        let admin2 = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin1);
        client.add_admin(&t.owner, &admin2);
        let prop_id = client.create_proposal(&admin1, &ProposalType::AddAdmin(target));
        client.execute_proposal(&admin2, &prop_id);

        let gov_sym: Symbol = symbol_short!("gov");
        let exec_sym: Symbol = symbol_short!("prop_exec");

        let exec_event = t.env
            .events()
            .all()
            .iter()
            .find(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(exec_sym.clone().into_val(&t.env))
            })
            .expect("prop_exec event missing");

        let data: ProposalExecutedEvent = exec_event.1.into_val(&t.env);
        assert_eq!(data.result, ProposalStatus::Rejected as u32);
        assert_eq!(data.votes_for, 0);
        assert_eq!(data.votes_against, 0);
    }

    // -----------------------------------------------------------------------
    // Event: AdminAddedEvent / AdminRemovedEvent via direct owner calls
    // -----------------------------------------------------------------------

    #[test]
    fn test_event_admin_added_direct() {
        let t = TestEnv::new();
        let client = t.client();
        let admin = Address::generate(&t.env);
        t.env.ledger().set_timestamp(77);
        client.add_admin(&t.owner, &admin);

        let gov_sym: Symbol = symbol_short!("gov");
        let adm_add_sym: Symbol = symbol_short!("adm_add");

        let evt = t.env
            .events()
            .all()
            .iter()
            .find(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(adm_add_sym.clone().into_val(&t.env))
            })
            .expect("adm_add event missing");

        let data: AdminAddedEvent = evt.1.into_val(&t.env);
        assert_eq!(data.admin, admin);
        assert_eq!(data.added_by, t.owner);
        assert_eq!(data.timestamp, 77);
    }

    #[test]
    fn test_event_admin_removed_direct() {
        let t = TestEnv::new();
        let client = t.client();
        let admin = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin);
        t.env.ledger().set_timestamp(88);
        client.remove_admin(&t.owner, &admin);

        let gov_sym: Symbol = symbol_short!("gov");
        let adm_rem_sym: Symbol = symbol_short!("adm_rem");

        let evt = t.env
            .events()
            .all()
            .iter()
            .find(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(adm_rem_sym.clone().into_val(&t.env))
            })
            .expect("adm_rem event missing");

        let data: AdminRemovedEvent = evt.1.into_val(&t.env);
        assert_eq!(data.admin, admin);
        assert_eq!(data.removed_by, t.owner);
        assert_eq!(data.timestamp, 88);
    }

    // -----------------------------------------------------------------------
    // Event: AdminAddedEvent / AdminRemovedEvent via proposal execution
    // -----------------------------------------------------------------------

    #[test]
    fn test_event_admin_added_via_proposal() {
        let t = TestEnv::new();
        let client = t.client();
        let admin = Address::generate(&t.env);
        let new_admin = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin);
        let prop_id = client.create_proposal(&admin, &ProposalType::AddAdmin(new_admin.clone()));
        client.vote(&admin, &prop_id, &true);
        client.execute_proposal(&admin, &prop_id);

        let gov_sym: Symbol = symbol_short!("gov");
        let adm_add_sym: Symbol = symbol_short!("adm_add");

        // Two adm_add events: one from add_admin, one from execute_proposal
        let count = t.env
            .events()
            .all()
            .iter()
            .filter(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(adm_add_sym.clone().into_val(&t.env))
            })
            .count();
        assert_eq!(count, 2);

        // The last adm_add event should be for new_admin
        let last_evt = t.env
            .events()
            .all()
            .iter()
            .filter(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(adm_add_sym.clone().into_val(&t.env))
            })
            .last()
            .unwrap();
        let data: AdminAddedEvent = last_evt.1.into_val(&t.env);
        assert_eq!(data.admin, new_admin);
        assert_eq!(data.added_by, admin);
    }

    #[test]
    fn test_event_admin_removed_via_proposal() {
        let t = TestEnv::new();
        let client = t.client();
        let admin1 = Address::generate(&t.env);
        let admin2 = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin1);
        client.add_admin(&t.owner, &admin2);

        let prop_id = client.create_proposal(&admin1, &ProposalType::RemoveAdmin(admin2.clone()));
        client.vote(&admin1, &prop_id, &true);
        client.execute_proposal(&admin1, &prop_id);

        assert!(!client.is_admin(&admin2));

        let gov_sym: Symbol = symbol_short!("gov");
        let adm_rem_sym: Symbol = symbol_short!("adm_rem");

        let evt = t.env
            .events()
            .all()
            .iter()
            .find(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(adm_rem_sym.clone().into_val(&t.env))
            })
            .expect("adm_rem event missing");

        let data: AdminRemovedEvent = evt.1.into_val(&t.env);
        assert_eq!(data.admin, admin2);
        assert_eq!(data.removed_by, admin1);
    }

    // -----------------------------------------------------------------------
    // No spurious events on error paths
    // -----------------------------------------------------------------------

    /// Verify that a successful single vote emits exactly one `voted` event,
    /// confirming no phantom emissions occur on the happy path.
    #[test]
    fn test_exactly_one_vote_event_per_cast() {
        let t = TestEnv::new();
        let client = t.client();
        let admin = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin);
        let prop_id = client.create_proposal(&admin, &ProposalType::AddAdmin(target));
        client.vote(&admin, &prop_id, &true);

        let gov_sym: Symbol = symbol_short!("gov");
        let voted_sym: Symbol = symbol_short!("voted");
        let count = t.env
            .events()
            .all()
            .iter()
            .filter(|e| {
                let topics = e.0.clone();
                topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                    && topics.get(1) == Some(voted_sym.clone().into_val(&t.env))
            })
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    #[should_panic(expected = "Already voted")]
    fn test_double_vote_no_second_event() {
        let t = TestEnv::new();
        let client = t.client();
        let admin = Address::generate(&t.env);
        let target = Address::generate(&t.env);
        client.add_admin(&t.owner, &admin);
        let prop_id = client.create_proposal(&admin, &ProposalType::AddAdmin(target));
        client.vote(&admin, &prop_id, &true);
        client.vote(&admin, &prop_id, &true);
    }

    // -----------------------------------------------------------------------
    // Integration: full proposal round-trip with event ordering
    // -----------------------------------------------------------------------

    #[test]
    fn test_full_round_trip_event_ordering() {
        let t = TestEnv::new();
        let client = t.client();
        let admin1 = Address::generate(&t.env);
        let admin2 = Address::generate(&t.env);
        let new_admin = Address::generate(&t.env);

        client.add_admin(&t.owner, &admin1);
        client.add_admin(&t.owner, &admin2);

        let prop_id = client.create_proposal(&admin1, &ProposalType::AddAdmin(new_admin.clone()));
        client.vote(&admin1, &prop_id, &true);
        client.vote(&admin2, &prop_id, &true);
        client.execute_proposal(&admin1, &prop_id);

        let gov_sym: Symbol = symbol_short!("gov");
        let all = t.env.events().all();

        // Collect second-topic symbols for all gov events, in emission order
        let topic1s: soroban_sdk::Vec<Symbol> = all
            .iter()
            .filter_map(|e| {
                let topics = e.0.clone();
                if topics.len() >= 2
                    && topics.get(0) == Some(gov_sym.clone().into_val(&t.env))
                {
                    topics.get(1).map(|v| v.into_val(&t.env))
                } else {
                    None
                }
            })
            .collect();

        // Expected: adm_add, adm_add, prop_new, voted, voted, adm_add (exec), prop_exec
        assert_eq!(topic1s.len(), 7);
        assert_eq!(topic1s.get(0).unwrap(), symbol_short!("adm_add"));
        assert_eq!(topic1s.get(1).unwrap(), symbol_short!("adm_add"));
        assert_eq!(topic1s.get(2).unwrap(), symbol_short!("prop_new"));
        assert_eq!(topic1s.get(3).unwrap(), symbol_short!("voted"));
        assert_eq!(topic1s.get(4).unwrap(), symbol_short!("voted"));
        assert_eq!(topic1s.get(5).unwrap(), symbol_short!("adm_add"));
        assert_eq!(topic1s.get(6).unwrap(), symbol_short!("prop_exec"));
    }
}
