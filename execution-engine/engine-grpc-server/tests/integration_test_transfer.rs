extern crate casperlabs_engine_grpc_server;
extern crate contract_ffi;
extern crate engine_core;
extern crate engine_shared;
extern crate engine_storage;
extern crate grpc;

use std::collections::HashMap;

use grpc::RequestOptions;

use casperlabs_engine_grpc_server::engine_server::ipc_grpc::ExecutionEngineService;
use contract_ffi::bytesrepr::ToBytes;
use contract_ffi::key::Key;
use contract_ffi::uref::{AccessRights, URef};
use contract_ffi::value::account::{PublicKey, PurseId};
use contract_ffi::value::{Value, U512};
use engine_core::engine_state::EngineState;
use engine_shared::transform::Transform;
use engine_storage::global_state::in_memory::InMemoryGlobalState;
use test_support::DEFAULT_BLOCK_TIME;

#[allow(unused)]
mod test_support;

const INITIAL_GENESIS_AMOUNT: u64 = 100_000_000_000;

const TRANSFER_1_AMOUNT: u32 = 1000;
const TRANSFER_2_AMOUNT: u32 = 750;

const GENESIS_ADDR: [u8; 32] = [6u8; 32];
const ACCOUNT_1_ADDR: [u8; 32] = [1u8; 32];
const ACCOUNT_2_ADDR: [u8; 32] = [2u8; 32];

// This value was acquired by observing the output of an execution of "create_purse_01.wasm"
// made by ACCOUNT_1.
const EXPECTED_UREF_BYTES: [u8; 32] = [
    0xb9, 0x8a, 0x1b, 0xee, 0xd7, 0x95, 0x99, 0x1f, 0x3a, 0x54, 0xdf, 0xb1, 0xad, 0xc8, 0x48, 0x0b,
    0x16, 0x20, 0x14, 0x25, 0x58, 0xb1, 0x4c, 0x09, 0x16, 0x1f, 0xf1, 0xe7, 0x69, 0xbd, 0x8f, 0xc9,
];

struct TestContext {
    mint_contract_uref: URef,
    locals: HashMap<PurseId, Key>,
}

impl TestContext {
    fn new(mint_contract_uref: URef) -> Self {
        TestContext {
            mint_contract_uref,
            locals: Default::default(),
        }
    }

    /// This method stores an association between a given purse_id and the
    /// the underlying balance uref associated with that purse id.  The balance uref
    /// is extracted from a given set of write transformations, using the local key
    /// generated by the mint contract's uref and the purse id.
    fn track(&mut self, transforms: &HashMap<Key, Transform>, purse_id: PurseId) {
        let local = {
            let purse_id_bytes = purse_id
                .value()
                .addr()
                .to_bytes()
                .expect("should serialize");
            Key::local(self.mint_contract_uref.addr(), &purse_id_bytes)
        };
        if let Some(Transform::Write(Value::Key(key @ Key::URef(_)))) = transforms.get(&local) {
            self.locals.insert(purse_id, key.normalize());
        }
    }

    fn lookup(&self, transforms: &HashMap<Key, Transform>, purse_id: PurseId) -> Option<Transform> {
        self.locals
            .get(&purse_id)
            .and_then(|local: &Key| transforms.get(local))
            .map(ToOwned::to_owned)
    }
}

#[ignore]
#[test]
fn should_transfer_to_account() {
    let initial_genesis_amount: U512 = U512::from(INITIAL_GENESIS_AMOUNT);
    let transfer_amount: U512 = U512::from(TRANSFER_1_AMOUNT);
    let genesis_account_key = Key::Account(GENESIS_ADDR);
    let account_key = Key::Account(ACCOUNT_1_ADDR);

    let global_state = InMemoryGlobalState::empty().unwrap();
    let engine_state = EngineState::new(global_state, Default::default());

    // Run genesis

    let (genesis_request, contracts) =
        test_support::create_genesis_request(GENESIS_ADDR, HashMap::new());

    let genesis_response = engine_state
        .run_genesis(RequestOptions::new(), genesis_request)
        .wait_drop_metadata()
        .unwrap();

    let genesis_hash = genesis_response.get_success().get_poststate_hash();

    let genesis_transforms = test_support::get_genesis_transforms(&genesis_response);

    let mint_contract_uref = test_support::get_mint_contract_uref(&genesis_transforms, &contracts)
        .expect("should get uref");

    let mut test_context = TestContext::new(mint_contract_uref);

    let genesis_account = test_support::get_account(&genesis_transforms, &genesis_account_key)
        .expect("should get account");

    let genesis_account_purse_id = genesis_account.purse_id();

    test_context.track(&genesis_transforms, genesis_account_purse_id);

    // Check genesis account balance

    let genesis_balance_transform = test_context
        .lookup(&genesis_transforms, genesis_account_purse_id)
        .expect("should lookup");

    assert_eq!(
        genesis_balance_transform,
        Transform::Write(Value::UInt512(initial_genesis_amount))
    );

    // Exec transfer contract

    let exec_request = test_support::create_exec_request(
        GENESIS_ADDR,
        "transfer_to_account_01.wasm",
        genesis_hash,
        DEFAULT_BLOCK_TIME,
        1,
        ACCOUNT_1_ADDR,
        vec![PublicKey::new(GENESIS_ADDR)],
    );

    let exec_response = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata()
        .unwrap();

    let exec_transforms = &test_support::get_exec_transforms(&exec_response)[0];

    let account =
        test_support::get_account(&exec_transforms, &account_key).expect("should get account");

    let account_purse_id = account.purse_id();

    test_context.track(&exec_transforms, account_purse_id);

    // Check genesis account balance

    let genesis_balance_transform = test_context
        .lookup(&exec_transforms, genesis_account_purse_id)
        .expect("should lookup");

    assert_eq!(
        genesis_balance_transform,
        Transform::Write(Value::UInt512(initial_genesis_amount - transfer_amount))
    );

    // Check account 1 balance

    let account_1_balance_transform = test_context
        .lookup(&exec_transforms, account_purse_id)
        .expect("should lookup");

    assert_eq!(
        account_1_balance_transform,
        Transform::Write(Value::UInt512(transfer_amount))
    );
}

#[ignore]
#[test]
fn should_transfer_from_account_to_account() {
    let initial_genesis_amount: U512 = U512::from(INITIAL_GENESIS_AMOUNT);
    let transfer_1_amount: U512 = U512::from(TRANSFER_1_AMOUNT);
    let transfer_2_amount: U512 = U512::from(TRANSFER_2_AMOUNT);
    let genesis_account_key = Key::Account(GENESIS_ADDR);
    let account_1_key = Key::Account(ACCOUNT_1_ADDR);
    let account_2_key = Key::Account(ACCOUNT_2_ADDR);

    let global_state = InMemoryGlobalState::empty().unwrap();
    let engine_state = EngineState::new(global_state, Default::default());

    // Run genesis

    let (genesis_request, contracts) =
        test_support::create_genesis_request(GENESIS_ADDR, HashMap::new());

    let genesis_response = engine_state
        .run_genesis(RequestOptions::new(), genesis_request)
        .wait_drop_metadata()
        .unwrap();

    let genesis_hash = genesis_response.get_success().get_poststate_hash();

    let genesis_transforms = test_support::get_genesis_transforms(&genesis_response);

    let mint_contract_uref = test_support::get_mint_contract_uref(&genesis_transforms, &contracts)
        .expect("should get uref");

    let mut test_context = TestContext::new(mint_contract_uref);

    let genesis_account = test_support::get_account(&genesis_transforms, &genesis_account_key)
        .expect("should get account");

    let genesis_account_purse_id = genesis_account.purse_id();

    test_context.track(&genesis_transforms, genesis_account_purse_id);

    // Exec transfer 1 contract

    let exec_request = test_support::create_exec_request(
        GENESIS_ADDR,
        "transfer_to_account_01.wasm",
        genesis_hash,
        DEFAULT_BLOCK_TIME,
        1,
        ACCOUNT_1_ADDR,
        vec![PublicKey::new(GENESIS_ADDR)],
    );

    let exec_1_response = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata()
        .unwrap();

    let exec_1_transforms = &test_support::get_exec_transforms(&exec_1_response)[0];

    let account_1 =
        test_support::get_account(&exec_1_transforms, &account_1_key).expect("should get account");

    let account_1_purse_id = account_1.purse_id();

    test_context.track(&exec_1_transforms, account_1_purse_id);

    // Check genesis account balance

    let genesis_balance_transform = test_context
        .lookup(&exec_1_transforms, genesis_account_purse_id)
        .expect("should lookup");

    assert_eq!(
        genesis_balance_transform,
        Transform::Write(Value::UInt512(initial_genesis_amount - transfer_1_amount))
    );

    // Check account 1 balance

    let account_1_balance_transform = test_context
        .lookup(&exec_1_transforms, account_1_purse_id)
        .expect("should lookup");

    assert_eq!(
        account_1_balance_transform,
        Transform::Write(Value::UInt512(transfer_1_amount))
    );

    // Commit transfer contract

    let commit_request = test_support::create_commit_request(genesis_hash, &exec_1_transforms);

    let commit_response = engine_state
        .commit(RequestOptions::new(), commit_request)
        .wait_drop_metadata()
        .unwrap();

    assert!(
        commit_response.has_success(),
        "Commit wasn't successful: {:?}",
        commit_response
    );

    let commit_hash = commit_response.get_success().get_poststate_hash();

    // Exec transfer 2 contract

    let exec_request = test_support::create_exec_request(
        ACCOUNT_1_ADDR,
        "transfer_to_account_02.wasm",
        commit_hash,
        DEFAULT_BLOCK_TIME,
        1,
        (),
        vec![PublicKey::new(ACCOUNT_1_ADDR)],
    );

    let exec_2_response = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata()
        .unwrap();

    let exec_2_transforms = &test_support::get_exec_transforms(&exec_2_response)[0];

    let account_2 =
        test_support::get_account(&exec_2_transforms, &account_2_key).expect("should get account");

    let account_2_purse_id = account_2.purse_id();

    test_context.track(&exec_2_transforms, account_2_purse_id);

    // Check account 1 balance

    let account_1_balance_transform = test_context
        .lookup(&exec_2_transforms, account_1_purse_id)
        .expect("should lookup");

    assert_eq!(
        account_1_balance_transform,
        Transform::Write(Value::UInt512(transfer_1_amount - transfer_2_amount))
    );

    let account_2_balance_transform = test_context
        .lookup(&exec_2_transforms, account_2_purse_id)
        .expect("should lookup");

    assert_eq!(
        account_2_balance_transform,
        Transform::Write(Value::UInt512(transfer_2_amount))
    );
}

#[ignore]
#[test]
fn should_transfer_to_existing_account() {
    let initial_genesis_amount: U512 = U512::from(INITIAL_GENESIS_AMOUNT);
    let transfer_1_amount: U512 = U512::from(TRANSFER_1_AMOUNT);
    let transfer_2_amount: U512 = U512::from(TRANSFER_2_AMOUNT);
    let genesis_account_key = Key::Account(GENESIS_ADDR);
    let account_1_key = Key::Account(ACCOUNT_1_ADDR);
    let account_2_key = Key::Account(ACCOUNT_2_ADDR);

    let global_state = InMemoryGlobalState::empty().unwrap();
    let engine_state = EngineState::new(global_state, Default::default());

    // Run genesis

    let (genesis_request, contracts) =
        test_support::create_genesis_request(GENESIS_ADDR, HashMap::new());

    let genesis_response = engine_state
        .run_genesis(RequestOptions::new(), genesis_request)
        .wait_drop_metadata()
        .unwrap();

    let genesis_hash = genesis_response.get_success().get_poststate_hash();

    let genesis_transforms = test_support::get_genesis_transforms(&genesis_response);

    let mint_contract_uref = test_support::get_mint_contract_uref(&genesis_transforms, &contracts)
        .expect("should get uref");

    let mut test_context = TestContext::new(mint_contract_uref);

    let genesis_account = test_support::get_account(&genesis_transforms, &genesis_account_key)
        .expect("should get account");

    let genesis_account_purse_id = genesis_account.purse_id();

    test_context.track(&genesis_transforms, genesis_account_purse_id);

    // Check genesis account balance

    let genesis_balance_transform = test_context
        .lookup(&genesis_transforms, genesis_account_purse_id)
        .expect("should lookup");

    assert_eq!(
        genesis_balance_transform,
        Transform::Write(Value::UInt512(initial_genesis_amount))
    );

    // Exec transfer contract

    let exec_request = test_support::create_exec_request(
        GENESIS_ADDR,
        "transfer_to_account_01.wasm",
        genesis_hash,
        DEFAULT_BLOCK_TIME,
        1,
        ACCOUNT_1_ADDR,
        vec![PublicKey::new(GENESIS_ADDR)],
    );

    let exec_response = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata()
        .unwrap();

    let exec_1_transforms = &test_support::get_exec_transforms(&exec_response)[0];

    let account_1 =
        test_support::get_account(&exec_1_transforms, &account_1_key).expect("should get account");

    let account_1_purse_id = account_1.purse_id();

    test_context.track(&exec_1_transforms, account_1_purse_id);

    // Check genesis account balance

    let genesis_balance_transform = test_context
        .lookup(&exec_1_transforms, genesis_account_purse_id)
        .expect("should lookup");

    assert_eq!(
        genesis_balance_transform,
        Transform::Write(Value::UInt512(initial_genesis_amount - transfer_1_amount))
    );

    // Check account 1 balance

    let account_1_balance_transform = test_context
        .lookup(&exec_1_transforms, account_1_purse_id)
        .expect("should lookup");

    assert_eq!(
        account_1_balance_transform,
        Transform::Write(Value::UInt512(transfer_1_amount))
    );

    // Commit transfer contract

    let commit_request = test_support::create_commit_request(genesis_hash, &exec_1_transforms);

    let commit_response = engine_state
        .commit(RequestOptions::new(), commit_request)
        .wait_drop_metadata()
        .unwrap();

    assert!(
        commit_response.has_success(),
        "Commit wasn't successful: {:?}",
        commit_response
    );

    let commit_hash = commit_response.get_success().get_poststate_hash();

    // Exec transfer contract

    let exec_request = test_support::create_exec_request(
        ACCOUNT_1_ADDR,
        "transfer_to_account_02.wasm",
        commit_hash,
        DEFAULT_BLOCK_TIME,
        1,
        (),
        vec![PublicKey::new(ACCOUNT_1_ADDR)],
    );

    let exec_response = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata()
        .unwrap();

    let exec_2_transforms = &test_support::get_exec_transforms(&exec_response)[0];

    let account_2 =
        test_support::get_account(&exec_2_transforms, &account_2_key).expect("should get account");

    let account_2_purse_id = account_2.purse_id();

    test_context.track(&exec_2_transforms, account_2_purse_id);

    // Check account 1 balance

    let account_1_balance_transform = test_context
        .lookup(&exec_2_transforms, account_1_purse_id)
        .expect("should lookup");

    assert_eq!(
        account_1_balance_transform,
        Transform::Write(Value::UInt512(transfer_1_amount - transfer_2_amount))
    );

    // Check account 2 balance

    let account_2_balance_transform = test_context
        .lookup(&exec_2_transforms, account_2_purse_id)
        .expect("should lookup");

    assert_eq!(
        account_2_balance_transform,
        Transform::Write(Value::UInt512(transfer_2_amount))
    );
}

#[ignore]
#[test]
fn should_fail_when_insufficient_funds() {
    let global_state = InMemoryGlobalState::empty().unwrap();
    let engine_state = EngineState::new(global_state, Default::default());

    // Run genesis

    let (genesis_request, _) = test_support::create_genesis_request(GENESIS_ADDR, HashMap::new());

    let genesis_response = engine_state
        .run_genesis(RequestOptions::new(), genesis_request)
        .wait_drop_metadata()
        .unwrap();

    let genesis_hash = genesis_response.get_success().get_poststate_hash();

    // Exec transfer contract

    let exec_request = test_support::create_exec_request(
        GENESIS_ADDR,
        "transfer_to_account_01.wasm",
        genesis_hash,
        DEFAULT_BLOCK_TIME,
        1,
        ACCOUNT_1_ADDR,
        vec![PublicKey::new(GENESIS_ADDR)],
    );

    let exec_response = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata()
        .unwrap();

    let exec_1_transforms = &test_support::get_exec_transforms(&exec_response)[0];

    // Commit transfer contract

    let commit_request = test_support::create_commit_request(genesis_hash, &exec_1_transforms);

    let commit_response = engine_state
        .commit(RequestOptions::new(), commit_request)
        .wait_drop_metadata()
        .unwrap();

    assert!(
        commit_response.has_success(),
        "Commit wasn't successful: {:?}",
        commit_response
    );

    let commit_hash = commit_response.get_success().get_poststate_hash();

    // Exec transfer contract

    let exec_request = test_support::create_exec_request(
        ACCOUNT_1_ADDR,
        "transfer_to_account_02.wasm",
        commit_hash,
        DEFAULT_BLOCK_TIME,
        1,
        (),
        vec![PublicKey::new(ACCOUNT_1_ADDR)],
    );

    let exec_response = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata()
        .unwrap();

    let exec_2_transforms = &test_support::get_exec_transforms(&exec_response)[0];

    // Commit transfer contract

    let commit_request = test_support::create_commit_request(commit_hash, &exec_2_transforms);

    let commit_response = engine_state
        .commit(RequestOptions::new(), commit_request)
        .wait_drop_metadata()
        .unwrap();

    let commit_hash = commit_response.get_success().get_poststate_hash();

    // Exec transfer contract

    let exec_request = test_support::create_exec_request(
        ACCOUNT_1_ADDR,
        "transfer_to_account_02.wasm",
        commit_hash,
        DEFAULT_BLOCK_TIME,
        2,
        (),
        vec![PublicKey::new(ACCOUNT_1_ADDR)],
    );

    let exec_response = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata()
        .unwrap();

    assert_eq!(
        "Trap(Trap { kind: Unreachable })",
        exec_response
            .get_success()
            .get_deploy_results()
            .get(0)
            .unwrap()
            .get_execution_result()
            .get_error()
            .get_exec_error()
            .get_message()
    )
}

#[ignore]
#[test]
fn should_create_purse() {
    let genesis_account_key = Key::Account(GENESIS_ADDR);
    let account_key = Key::Account(ACCOUNT_1_ADDR);
    let global_state = InMemoryGlobalState::empty().unwrap();
    let engine_state = EngineState::new(global_state, Default::default());

    // Run genesis & set up an account

    let (genesis_request, contracts) =
        test_support::create_genesis_request(GENESIS_ADDR, HashMap::new());

    let genesis_response = engine_state
        .run_genesis(RequestOptions::new(), genesis_request)
        .wait_drop_metadata()
        .unwrap();

    let genesis_hash = genesis_response.get_success().get_poststate_hash();

    let genesis_transforms = test_support::get_genesis_transforms(&genesis_response);

    let mint_contract_uref = test_support::get_mint_contract_uref(&genesis_transforms, &contracts)
        .expect("should get uref");

    let mut test_context = TestContext::new(mint_contract_uref);

    let genesis_account = test_support::get_account(&genesis_transforms, &genesis_account_key)
        .expect("should get account");

    let genesis_account_purse_id = genesis_account.purse_id();

    test_context.track(&genesis_transforms, genesis_account_purse_id);

    // Exec transfer

    let exec_request = test_support::create_exec_request(
        GENESIS_ADDR,
        "transfer_to_account_01.wasm",
        genesis_hash,
        DEFAULT_BLOCK_TIME,
        1,
        ACCOUNT_1_ADDR,
        vec![PublicKey::new(GENESIS_ADDR)],
    );

    let exec_response = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata()
        .unwrap();

    let exec_transforms = &test_support::get_exec_transforms(&exec_response)[0];

    let account =
        test_support::get_account(&exec_transforms, &account_key).expect("should get account");

    let account_purse_id = account.purse_id();

    test_context.track(&exec_transforms, account_purse_id);

    // Commit

    let commit_request = test_support::create_commit_request(genesis_hash, &exec_transforms);

    let commit_response = engine_state
        .commit(RequestOptions::new(), commit_request)
        .wait_drop_metadata()
        .unwrap();

    assert!(
        commit_response.has_success(),
        "Commit wasn't successful: {:?}",
        commit_response
    );

    let commit_hash = commit_response.get_success().get_poststate_hash();

    // Create purse

    let exec_request = test_support::create_exec_request(
        ACCOUNT_1_ADDR,
        "create_purse_01.wasm",
        commit_hash,
        DEFAULT_BLOCK_TIME,
        1,
        (),
        vec![PublicKey::new(ACCOUNT_1_ADDR)],
    );

    let exec_response = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata()
        .unwrap();

    let exec_transforms = &test_support::get_exec_transforms(&exec_response)[0];

    let expected_purse_id = PurseId::new(
        URef::new(EXPECTED_UREF_BYTES, AccessRights::READ_ADD_WRITE).remove_access_rights(),
    );
    test_context.track(&exec_transforms, expected_purse_id);

    let account = &exec_transforms
        [&Key::URef(URef::new(EXPECTED_UREF_BYTES, AccessRights::READ_ADD_WRITE)).normalize()];
    assert_eq!(account, &Transform::Write(Value::UInt512(U512::from(0))));
}
