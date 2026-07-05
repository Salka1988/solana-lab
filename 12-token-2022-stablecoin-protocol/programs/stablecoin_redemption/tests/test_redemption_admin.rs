use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const REDEMPTION_AMOUNT: u64 = 1_000;

fn setup() -> (LiteSVM, Keypair) {
    test_support::new_svm_with_program(
        stablecoin_redemption::id(),
        include_bytes!("../../../target/deploy/stablecoin_redemption.so"),
    )
}

fn protocol_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[stablecoin_redemption::PROTOCOL_SEED],
        &stablecoin_redemption::id(),
    )
}

fn redemption_vault_pda(protocol_config: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            stablecoin_redemption::REDEMPTION_VAULT_SEED,
            protocol_config.as_ref(),
        ],
        &stablecoin_redemption::id(),
    )
}

fn redemption_request_pda(protocol_config: Pubkey, request_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            stablecoin_redemption::REDEMPTION_REQUEST_SEED,
            protocol_config.as_ref(),
            &request_id.to_le_bytes(),
        ],
        &stablecoin_redemption::id(),
    )
}

fn admin_action_log_pda(protocol_config: Pubkey, action_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            stablecoin_redemption::ADMIN_ACTION_LOG_SEED,
            protocol_config.as_ref(),
            &action_id.to_le_bytes(),
        ],
        &stablecoin_redemption::id(),
    )
}

fn initialize_protocol_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    redemption_vault: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_redemption::id(),
        &stablecoin_redemption::instruction::InitializeProtocol {}.data(),
        stablecoin_redemption::accounts::InitializeProtocol {
            admin,
            protocol_config,
            redemption_vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn request_redemption_ix(
    owner: Pubkey,
    protocol_config: Pubkey,
    redemption_vault: Pubkey,
    redemption_request: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_redemption::id(),
        &stablecoin_redemption::instruction::RequestRedemption { amount }.data(),
        stablecoin_redemption::accounts::RequestRedemption {
            owner,
            protocol_config,
            redemption_vault,
            redemption_request,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn cancel_redemption_ix(
    owner: Pubkey,
    protocol_config: Pubkey,
    redemption_vault: Pubkey,
    redemption_request: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_redemption::id(),
        &stablecoin_redemption::instruction::CancelRedemption {}.data(),
        stablecoin_redemption::accounts::CancelRedemption {
            owner,
            protocol_config,
            redemption_vault,
            redemption_request,
        }
        .to_account_metas(None),
    )
}

fn complete_redemption_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    redemption_vault: Pubkey,
    redemption_request: Pubkey,
    admin_action_log: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_redemption::id(),
        &stablecoin_redemption::instruction::CompleteRedemption {}.data(),
        stablecoin_redemption::accounts::AdminRedemptionAction {
            admin,
            protocol_config,
            redemption_vault,
            redemption_request,
            admin_action_log,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn reject_redemption_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    redemption_vault: Pubkey,
    redemption_request: Pubkey,
    admin_action_log: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_redemption::id(),
        &stablecoin_redemption::instruction::RejectRedemption {}.data(),
        stablecoin_redemption::accounts::AdminRedemptionAction {
            admin,
            protocol_config,
            redemption_vault,
            redemption_request,
            admin_action_log,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn set_redemptions_paused_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    admin_action_log: Pubkey,
    paused: bool,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_redemption::id(),
        &stablecoin_redemption::instruction::SetRedemptionsPaused { paused }.data(),
        stablecoin_redemption::accounts::SetRedemptionsPaused {
            admin,
            protocol_config,
            admin_action_log,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn begin_admin_transfer_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    admin_action_log: Pubkey,
    new_admin: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_redemption::id(),
        &stablecoin_redemption::instruction::BeginAdminTransfer { new_admin }.data(),
        stablecoin_redemption::accounts::BeginAdminTransfer {
            admin,
            protocol_config,
            admin_action_log,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn accept_admin_transfer_ix(
    pending_admin: Pubkey,
    protocol_config: Pubkey,
    admin_action_log: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_redemption::id(),
        &stablecoin_redemption::instruction::AcceptAdminTransfer {}.data(),
        stablecoin_redemption::accounts::AcceptAdminTransfer {
            pending_admin,
            protocol_config,
            admin_action_log,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

struct Fixture {
    svm: LiteSVM,
    admin: Keypair,
    protocol_config: Pubkey,
    redemption_vault: Pubkey,
}

fn initialized_fixture() -> Fixture {
    let (mut svm, admin) = setup();
    let (protocol_config, _) = protocol_config_pda();
    let (redemption_vault, _) = redemption_vault_pda(protocol_config);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config, redemption_vault),
    ));

    Fixture {
        svm,
        admin,
        protocol_config,
        redemption_vault,
    }
}

fn create_request(fixture: &mut Fixture, owner: &Keypair, amount: u64) -> Pubkey {
    let state = test_support::deserialize_account::<stablecoin_redemption::ProtocolConfig>(
        &fixture.svm,
        &fixture.protocol_config,
    );
    let (request, _) = redemption_request_pda(fixture.protocol_config, state.next_request_id);

    assert!(test_support::send_instruction(
        &mut fixture.svm,
        owner,
        request_redemption_ix(
            owner.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
            amount,
        ),
    ));

    request
}

#[test]
fn initialize_protocol_creates_config_and_vault() {
    let (mut svm, admin) = setup();
    let (protocol_config, expected_protocol_bump) = protocol_config_pda();
    let (redemption_vault, expected_vault_bump) = redemption_vault_pda(protocol_config);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config, redemption_vault),
    ));

    let config = test_support::deserialize_account::<stablecoin_redemption::ProtocolConfig>(
        &svm,
        &protocol_config,
    );
    let vault = test_support::deserialize_account::<stablecoin_redemption::RedemptionVault>(
        &svm,
        &redemption_vault,
    );

    assert_eq!(config.admin, admin.pubkey());
    assert_eq!(config.pending_admin, Pubkey::default());
    assert!(!config.redemptions_paused);
    assert_eq!(config.next_request_id, 0);
    assert_eq!(config.next_admin_action_id, 0);
    assert_eq!(config.bump, expected_protocol_bump);

    assert_eq!(vault.protocol_config, protocol_config);
    assert_eq!(vault.outstanding_amount, 0);
    assert_eq!(vault.total_requested, 0);
    assert_eq!(vault.bump, expected_vault_bump);
}

#[test]
fn request_redemption_creates_pending_request_and_updates_vault() {
    let mut fixture = initialized_fixture();
    let owner = Keypair::new();
    test_support::fund_user(&mut fixture.svm, &owner);

    let request = create_request(&mut fixture, &owner, REDEMPTION_AMOUNT);

    let request_state = test_support::deserialize_account::<stablecoin_redemption::RedemptionRequest>(
        &fixture.svm,
        &request,
    );
    let vault = test_support::deserialize_account::<stablecoin_redemption::RedemptionVault>(
        &fixture.svm,
        &fixture.redemption_vault,
    );
    let config = test_support::deserialize_account::<stablecoin_redemption::ProtocolConfig>(
        &fixture.svm,
        &fixture.protocol_config,
    );

    assert_eq!(request_state.owner, owner.pubkey());
    assert_eq!(request_state.amount, REDEMPTION_AMOUNT);
    assert_eq!(
        request_state.status,
        stablecoin_redemption::RedemptionStatus::Pending
    );
    assert_eq!(vault.outstanding_amount, REDEMPTION_AMOUNT);
    assert_eq!(vault.total_requested, REDEMPTION_AMOUNT);
    assert_eq!(config.next_request_id, 1);
}

#[test]
fn request_redemption_rejects_zero_amount_and_paused_protocol() {
    let mut fixture = initialized_fixture();
    let owner = Keypair::new();
    test_support::fund_user(&mut fixture.svm, &owner);
    let (request, _) = redemption_request_pda(fixture.protocol_config, 0);

    let zero_result = test_support::send_instruction_result(
        &mut fixture.svm,
        &owner,
        request_redemption_ix(
            owner.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
            0,
        ),
    );
    test_support::assert_result_fails_with(zero_result, "InvalidRedemptionAmount");

    let (pause_log, _) = admin_action_log_pda(fixture.protocol_config, 0);
    assert!(test_support::send_instruction(
        &mut fixture.svm,
        &fixture.admin,
        set_redemptions_paused_ix(
            fixture.admin.pubkey(),
            fixture.protocol_config,
            pause_log,
            true
        ),
    ));

    let paused_result = test_support::send_instruction_result(
        &mut fixture.svm,
        &owner,
        request_redemption_ix(
            owner.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
            REDEMPTION_AMOUNT,
        ),
    );
    test_support::assert_result_fails_with(paused_result, "RedemptionsPaused");
}

#[test]
fn cancel_redemption_requires_owner_and_prevents_replay() {
    let mut fixture = initialized_fixture();
    let owner = Keypair::new();
    let attacker = Keypair::new();
    test_support::fund_user(&mut fixture.svm, &owner);
    test_support::fund_user(&mut fixture.svm, &attacker);
    let request = create_request(&mut fixture, &owner, REDEMPTION_AMOUNT);

    let attacker_result = test_support::send_instruction_result(
        &mut fixture.svm,
        &attacker,
        cancel_redemption_ix(
            attacker.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
        ),
    );
    test_support::assert_failure_contains(
        &attacker_result.unwrap_err(),
        "UnauthorizedRequestOwner",
    );

    assert!(test_support::send_instruction(
        &mut fixture.svm,
        &owner,
        cancel_redemption_ix(
            owner.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
        ),
    ));

    let replay_result = test_support::send_instruction_result(
        &mut fixture.svm,
        &owner,
        cancel_redemption_ix(
            owner.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
        ),
    );
    assert!(replay_result.is_err());

    let (admin_log, _) = admin_action_log_pda(fixture.protocol_config, 0);
    let settle_result = test_support::send_instruction_result(
        &mut fixture.svm,
        &fixture.admin,
        complete_redemption_ix(
            fixture.admin.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
            admin_log,
        ),
    );
    test_support::assert_result_fails_with(settle_result, "RequestNotPending");
}

#[test]
fn complete_redemption_updates_request_vault_and_admin_log() {
    let mut fixture = initialized_fixture();
    let owner = Keypair::new();
    test_support::fund_user(&mut fixture.svm, &owner);
    let request = create_request(&mut fixture, &owner, REDEMPTION_AMOUNT);
    let (admin_log, _) = admin_action_log_pda(fixture.protocol_config, 0);

    assert!(test_support::send_instruction(
        &mut fixture.svm,
        &fixture.admin,
        complete_redemption_ix(
            fixture.admin.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
            admin_log,
        ),
    ));

    let request_state = test_support::deserialize_account::<stablecoin_redemption::RedemptionRequest>(
        &fixture.svm,
        &request,
    );
    let vault = test_support::deserialize_account::<stablecoin_redemption::RedemptionVault>(
        &fixture.svm,
        &fixture.redemption_vault,
    );
    let log = test_support::deserialize_account::<stablecoin_redemption::AdminActionLog>(
        &fixture.svm,
        &admin_log,
    );

    assert_eq!(
        request_state.status,
        stablecoin_redemption::RedemptionStatus::Completed
    );
    assert_eq!(vault.outstanding_amount, 0);
    assert_eq!(vault.total_completed, REDEMPTION_AMOUNT);
    assert_eq!(
        log.action,
        stablecoin_redemption::AdminAction::CompleteRedemption
    );
    assert_eq!(log.target, request);
    assert_eq!(log.amount, REDEMPTION_AMOUNT);
}

#[test]
fn reject_redemption_updates_request_vault_and_admin_log() {
    let mut fixture = initialized_fixture();
    let owner = Keypair::new();
    test_support::fund_user(&mut fixture.svm, &owner);
    let request = create_request(&mut fixture, &owner, REDEMPTION_AMOUNT);
    let (admin_log, _) = admin_action_log_pda(fixture.protocol_config, 0);

    assert!(test_support::send_instruction(
        &mut fixture.svm,
        &fixture.admin,
        reject_redemption_ix(
            fixture.admin.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
            admin_log,
        ),
    ));

    let request_state = test_support::deserialize_account::<stablecoin_redemption::RedemptionRequest>(
        &fixture.svm,
        &request,
    );
    let vault = test_support::deserialize_account::<stablecoin_redemption::RedemptionVault>(
        &fixture.svm,
        &fixture.redemption_vault,
    );

    assert_eq!(
        request_state.status,
        stablecoin_redemption::RedemptionStatus::Rejected
    );
    assert_eq!(vault.outstanding_amount, 0);
    assert_eq!(vault.total_rejected, REDEMPTION_AMOUNT);
}

#[test]
fn admin_actions_reject_non_admin_and_wrong_log_replay() {
    let mut fixture = initialized_fixture();
    let owner = Keypair::new();
    let attacker = Keypair::new();
    test_support::fund_user(&mut fixture.svm, &owner);
    test_support::fund_user(&mut fixture.svm, &attacker);
    let request = create_request(&mut fixture, &owner, REDEMPTION_AMOUNT);
    let (admin_log, _) = admin_action_log_pda(fixture.protocol_config, 0);

    let attacker_result = test_support::send_instruction_result(
        &mut fixture.svm,
        &attacker,
        complete_redemption_ix(
            attacker.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
            admin_log,
        ),
    );
    test_support::assert_result_fails_with(attacker_result, "UnauthorizedAdmin");

    assert!(test_support::send_instruction(
        &mut fixture.svm,
        &fixture.admin,
        complete_redemption_ix(
            fixture.admin.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
            admin_log,
        ),
    ));

    let replay_result = test_support::send_instruction_result(
        &mut fixture.svm,
        &fixture.admin,
        complete_redemption_ix(
            fixture.admin.pubkey(),
            fixture.protocol_config,
            fixture.redemption_vault,
            request,
            admin_log,
        ),
    );
    assert!(replay_result.is_err());
}

#[test]
fn two_step_admin_transfer_requires_pending_admin() {
    let mut fixture = initialized_fixture();
    let new_admin = Keypair::new();
    let wrong_admin = Keypair::new();
    test_support::fund_user(&mut fixture.svm, &new_admin);
    test_support::fund_user(&mut fixture.svm, &wrong_admin);
    let (begin_log, _) = admin_action_log_pda(fixture.protocol_config, 0);
    let (accept_log, _) = admin_action_log_pda(fixture.protocol_config, 1);

    assert!(test_support::send_instruction(
        &mut fixture.svm,
        &fixture.admin,
        begin_admin_transfer_ix(
            fixture.admin.pubkey(),
            fixture.protocol_config,
            begin_log,
            new_admin.pubkey(),
        ),
    ));

    let wrong_result = test_support::send_instruction_result(
        &mut fixture.svm,
        &wrong_admin,
        accept_admin_transfer_ix(wrong_admin.pubkey(), fixture.protocol_config, accept_log),
    );
    test_support::assert_result_fails_with(wrong_result, "UnauthorizedPendingAdmin");

    assert!(test_support::send_instruction(
        &mut fixture.svm,
        &new_admin,
        accept_admin_transfer_ix(new_admin.pubkey(), fixture.protocol_config, accept_log),
    ));

    let config = test_support::deserialize_account::<stablecoin_redemption::ProtocolConfig>(
        &fixture.svm,
        &fixture.protocol_config,
    );
    assert_eq!(config.admin, new_admin.pubkey());
    assert_eq!(config.pending_admin, Pubkey::default());
    assert_eq!(config.next_admin_action_id, 2);
}
