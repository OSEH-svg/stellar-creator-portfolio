#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token::Client as TokenClient, Address, Env,
};

#[derive(Clone, Copy, PartialEq)]
#[contracttype]
pub enum EscrowStatus {
    Active = 0,
    Released = 1,
    Refunded = 2,
    Disputed = 3,
    EmergencyWithdrawn = 4,
}

#[derive(Clone)]
#[contracttype]
pub enum ReleaseCondition {
    OnCompletion,
    Timelock(u64),
}

#[contracttype]
pub struct EscrowAccount {
    pub id: u64,
    pub payer: Address,
    pub payee: Address,
    pub amount: i128,
    pub token: Address,
    pub status: EscrowStatus,
    pub release_condition: ReleaseCondition,
    pub created_at: u64,
}

#[contracttype]
pub enum DataKey {
    EscrowCounter,
    Escrow(u64),
    Governance,
}

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Creates and funds a new escrow account.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `payer`: Payer address (must authenticate and have sufficient balance).
    /// - `payee`: Recipient address.
    /// - `amount`: Amount to escrow (must be positive).
    /// - `token`: Token contract address for the escrow.
    /// - `release_condition`: Condition for fund release (`OnCompletion` or `Timelock`).
    ///
    /// # Returns
    /// - `u64`: Unique escrow ID.
    ///
    /// # Errors
    /// - Panics if payer fails authentication.
    /// - Panics if amount <= 0.
    /// - Token transfer will fail if insufficient balance/approval.
    ///
    /// # State Changes
    /// - Transfers tokens from payer to contract.
    /// - Increments escrow counter.
    /// - Stores EscrowAccount with `Active` status.
    pub fn deposit(
        env: Env,
        payer: Address,
        payee: Address,
        amount: i128,
        token: Address,
        release_condition: ReleaseCondition,
    ) -> u64 {
        payer.require_auth();
        assert!(amount > 0, \"Amount must be positive\");

        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&payer, &env.current_contract_address(), &amount);

        let mut counter: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::EscrowCounter)
            .unwrap_or(0);
        counter += 1;

        let escrow = EscrowAccount {
            id: counter,
            payer,
            payee,
            amount,
            token,
            status: EscrowStatus::Active,
            release_condition,
            created_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Escrow(counter), &escrow);
        env.storage()
            .persistent()
            .set(&DataKey::EscrowCounter, &counter);

        counter
    }

    /// Retrieves escrow account details by ID.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `escrow_id`: Unique escrow ID.
    ///
    /// # Returns
    /// - `EscrowAccount`: Full escrow details.
    ///
    /// # Errors
    /// - Panics with \"Escrow not found\" if ID doesn't exist.
    pub fn get_escrow(env: Env, escrow_id: u64) -> EscrowAccount {
        env.storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .expect(\"Escrow not found\")
    }

    /// Releases escrowed funds to payee if conditions met.
    /// Can be called by payer or payee.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `escrow_id`: Escrow ID.
    /// - `caller`: Caller address (must be payer or payee, authenticates).
    ///
    /// # Returns
    /// - `bool`: Always `true` on success.
    ///
    /// # Errors
    /// - Panics if escrow not found or not active.
    /// - Panics if caller unauthorized (not payer/payee).
    /// - Panics if release condition not satisfied.
    /// - Token transfer fails if issues.
    ///
    /// # State Changes
    /// - Transfers full amount to payee.
    /// - Updates status to `Released`.
    pub fn release_funds(env: Env, escrow_id: u64, caller: Address) -> bool {
        caller.require_auth();

        let mut escrow: EscrowAccount = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .expect(\"Escrow not found\");

        assert!(
            caller == escrow.payer || caller == escrow.payee,
            \"Unauthorized\"
        );
        assert!(escrow.status == EscrowStatus::Active, \"Escrow not active\");
        assert!(
            Self::can_release(env.clone(), escrow_id),
            \"Release condition not met\"
        );

        let token_client = TokenClient::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.payee,
            &escrow.amount,
        );

        escrow.status = EscrowStatus::Released;
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        true
    }

    /// Refunds escrow to payer (payer only).
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `escrow_id`: Escrow ID.
    ///
    /// # Returns
    /// - `bool`: Always `true` on success.
    ///
    /// # Errors
    /// - Panics if escrow not found or not active.
    /// - Panics if payer fails authentication.
    ///
    /// # State Changes
    /// - Transfers full amount back to payer.
    /// - Updates status to `Refunded`.
    pub fn refund_escrow(env: Env, escrow_id: u64) -> bool {
        let mut escrow: EscrowAccount = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .expect(\"Escrow not found\");

        escrow.payer.require_auth();
        assert!(escrow.status == EscrowStatus::Active, \"Escrow not active\");

        let token_client = TokenClient::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.payer,
            &escrow.amount,
        );

        escrow.status = EscrowStatus::Refunded;
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);

        true
    }

    /// Checks if escrow release conditions are met.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `escrow_id`: Escrow ID.
    ///
    /// # Returns
    /// - `bool`: `true` if releasable.
    ///
    /// # Errors
    /// - Panics if escrow not found.
    ///
    /// # Logic
    /// - `OnCompletion`: Always true.
    /// - `Timelock(deadline)`: True if current timestamp >= deadline.
    pub fn can_release(env: Env, escrow_id: u64) -> bool {
        let escrow: EscrowAccount = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .expect(\"Escrow not found\");

        match escrow.release_condition {
            ReleaseCondition::OnCompletion => true,
            ReleaseCondition::Timelock(deadline) => env.ledger().timestamp() >= deadline,
        }
    }

    /// Gets total number of escrows created.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    ///
    /// # Returns
    /// - `u64`: Escrow count.
    pub fn get_escrow_count(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::EscrowCounter)
            .unwrap_or(0)
    }

    /// Sets the governance contract address.
    /// Can only be called once by an authorized caller.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `caller`: Authorized caller.
    /// - `governance`: Governance contract address.
    ///
    /// # Returns
    /// - `bool`: Always `true` on success.
    ///
    /// # Errors
    /// - Panics if caller fails authentication.
    /// - Panics if governance already set.
    pub fn set_governance(env: Env, caller: Address, governance: Address) -> bool {
        caller.require_auth();
        if env.storage().persistent().has(&DataKey::Governance) {
            panic!("Governance already set");
        }
        env.storage().persistent().set(&DataKey::Governance, &governance);
        true
    }

    /// Emergency withdrawal of stuck funds from disputed escrow.
    /// Only callable by governance admin.
    ///
    /// # Parameters
    /// - `env`: Soroban environment.
    /// - `caller`: Caller address (must be admin).
    /// - `escrow_id`: Escrow ID.
    /// - `recipient`: Address to receive the funds.
    ///
    /// # Returns
    /// - `bool`: Always `true` on success.
    ///
    /// # Errors
    /// - Panics if caller not authenticated.
    /// - Panics if caller not admin.
    /// - Panics if escrow not found or not disputed.
    /// - Token transfer fails if issues.
    ///
    /// # State Changes
    /// - Transfers full amount to recipient.
    /// - Updates status to `EmergencyWithdrawn`.
    /// - Emits event.
    pub fn emergency_withdraw(env: Env, caller: Address, escrow_id: u64, recipient: Address) -> bool {
        caller.require_auth();

        let governance: Address = env.storage().persistent().get(&DataKey::Governance).expect("Governance not set");

        let is_admin: bool = env.invoke_contract(
            &governance,
            &symbol_short!("is_admin"),
            (caller.clone(),).into_val(&env),
        );

        assert!(is_admin, "Unauthorized: not an admin");

        let mut escrow: EscrowAccount = env.storage().persistent().get(&DataKey::Escrow(escrow_id)).expect("Escrow not found");

        assert!(escrow.status == EscrowStatus::Disputed, "Can only emergency withdraw disputed escrows");

        let token_client = TokenClient::new(&env, &escrow.token);
        token_client.transfer(&env.current_contract_address(), &recipient, &escrow.amount);

        escrow.status = EscrowStatus::EmergencyWithdrawn;
        env.storage().persistent().set(&DataKey::Escrow(escrow_id), &escrow);

        env.events().publish((symbol_short!("emergency_withdraw"), escrow_id), (recipient, escrow.amount));

        true
    }
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn test_escrow_count_starts_at_zero() {
        let env = Env::default();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);
        assert_eq!(client.get_escrow_count(), 0);
    }

    #[test]
    fn test_emergency_withdraw_success() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);

        let governance = Address::generate(&env);
        let admin = Address::generate(&env);
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = env.register_stellar_asset_contract(Address::generate(&env));
        let amount = 1000_i128;

        // Set governance
        client.set_governance(&admin, &governance);

        // Mock is_admin to return true
        env.mock_contract(&governance, |mock| {
            mock.with_args(("is_admin", admin.clone())).returns(true);
        });

        // Deposit
        let escrow_id = client.deposit(&payer, &payee, &amount, &token, &ReleaseCondition::OnCompletion);

        // Set status to Disputed
        env.as_contract(&contract_id, || {
            let mut escrow: EscrowAccount = env.storage().persistent().get(&DataKey::Escrow(escrow_id)).unwrap();
            escrow.status = EscrowStatus::Disputed;
            env.storage().persistent().set(&DataKey::Escrow(escrow_id), &escrow);
        });

        // Emergency withdraw
        assert!(client.emergency_withdraw(&admin, &escrow_id, &payee));

        // Check status
        let escrow = client.get_escrow(&escrow_id);
        assert_eq!(escrow.status, EscrowStatus::EmergencyWithdrawn);
    }

    #[test]
    #[should_panic(expected = "Unauthorized: not an admin")]
    fn test_emergency_withdraw_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);

        let governance = Address::generate(&env);
        let admin = Address::generate(&env);
        let rando = Address::generate(&env);
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = env.register_stellar_asset_contract(Address::generate(&env));
        let amount = 1000_i128;

        client.set_governance(&admin, &governance);

        // Mock is_admin to return false for rando
        env.mock_contract(&governance, |mock| {
            mock.with_args(("is_admin", rando.clone())).returns(false);
        });

        let escrow_id = client.deposit(&payer, &payee, &amount, &token, &ReleaseCondition::OnCompletion);

        env.as_contract(&contract_id, || {
            let mut escrow: EscrowAccount = env.storage().persistent().get(&DataKey::Escrow(escrow_id)).unwrap();
            escrow.status = EscrowStatus::Disputed;
            env.storage().persistent().set(&DataKey::Escrow(escrow_id), &escrow);
        });

        // Should panic
        client.emergency_withdraw(&rando, &escrow_id, &payee);
    }

    #[test]
    #[should_panic(expected = "Can only emergency withdraw disputed escrows")]
    fn test_emergency_withdraw_not_disputed() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(&env, &contract_id);

        let governance = Address::generate(&env);
        let admin = Address::generate(&env);
        let payer = Address::generate(&env);
        let payee = Address::generate(&env);
        let token = env.register_stellar_asset_contract(Address::generate(&env));
        let amount = 1000_i128;

        client.set_governance(&admin, &governance);

        env.mock_contract(&governance, |mock| {
            mock.with_args(("is_admin", admin.clone())).returns(true);
        });

        let escrow_id = client.deposit(&payer, &payee, &amount, &token, &ReleaseCondition::OnCompletion);

        // Status is Active, not Disputed
        // Should panic
        client.emergency_withdraw(&admin, &escrow_id, &payee);
    }
}
