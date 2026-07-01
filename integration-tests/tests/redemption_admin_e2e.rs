use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    redemption_admin as redemption_program,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const REDEMPTION_AMOUNT: u64 = 5_000;

fn setup() -> (LiteSVM, Keypair) {
    let (mut svm, payer) = test_support::new_svm_with_payer();
    let redemption_bytes =
        include_bytes!("../../08-redemption-and-admin/target/deploy/redemption_admin.so");

    test_support::add_program(&mut svm, redemption_program::id(), redemption_bytes);

    (svm, payer)
}

fn protocol_config_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[redemption_program::PROTOCOL_SEED],
        &redemption_program::id(),
    )
    .0
}

fn redemption_vault_pda(protocol_config: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            redemption_program::REDEMPTION_VAULT_SEED,
            protocol_config.as_ref(),
        ],
        &redemption_program::id(),
    )
    .0
}

fn redemption_request_pda(protocol_config: Pubkey, request_id: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            redemption_program::REDEMPTION_REQUEST_SEED,
            protocol_config.as_ref(),
            &request_id.to_le_bytes(),
        ],
        &redemption_program::id(),
    )
    .0
}

fn admin_action_log_pda(protocol_config: Pubkey, action_id: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            redemption_program::ADMIN_ACTION_LOG_SEED,
            protocol_config.as_ref(),
            &action_id.to_le_bytes(),
        ],
        &redemption_program::id(),
    )
    .0
}

fn initialize_protocol_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    redemption_vault: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        redemption_program::id(),
        &redemption_program::instruction::InitializeProtocol {}.data(),
        redemption_program::accounts::InitializeProtocol {
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
        redemption_program::id(),
        &redemption_program::instruction::RequestRedemption { amount }.data(),
        redemption_program::accounts::RequestRedemption {
            owner,
            protocol_config,
            redemption_vault,
            redemption_request,
            system_program: system_program::ID,
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
        redemption_program::id(),
        &redemption_program::instruction::CompleteRedemption {}.data(),
        redemption_program::accounts::AdminRedemptionAction {
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

#[test]
fn user_requests_redemption_and_admin_completes_with_audit_log() {
    let (mut svm, admin) = setup();
    let user = Keypair::new();
    test_support::fund_user(&mut svm, &user);

    let protocol_config = protocol_config_pda();
    let redemption_vault = redemption_vault_pda(protocol_config);
    let redemption_request = redemption_request_pda(protocol_config, 0);
    let admin_action_log = admin_action_log_pda(protocol_config, 0);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config, redemption_vault),
    ));

    assert!(test_support::send_instruction(
        &mut svm,
        &user,
        request_redemption_ix(
            user.pubkey(),
            protocol_config,
            redemption_vault,
            redemption_request,
            REDEMPTION_AMOUNT,
        ),
    ));

    let pending_request = test_support::deserialize_account::<redemption_program::RedemptionRequest>(
        &svm,
        &redemption_request,
    );
    let pending_vault = test_support::deserialize_account::<redemption_program::RedemptionVault>(
        &svm,
        &redemption_vault,
    );

    assert_eq!(
        pending_request.status,
        redemption_program::RedemptionStatus::Pending
    );
    assert_eq!(pending_request.owner, user.pubkey());
    assert_eq!(pending_request.amount, REDEMPTION_AMOUNT);
    assert_eq!(pending_vault.outstanding_amount, REDEMPTION_AMOUNT);
    assert_eq!(pending_vault.total_requested, REDEMPTION_AMOUNT);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        complete_redemption_ix(
            admin.pubkey(),
            protocol_config,
            redemption_vault,
            redemption_request,
            admin_action_log,
        ),
    ));

    let completed_request = test_support::deserialize_account::<
        redemption_program::RedemptionRequest,
    >(&svm, &redemption_request);
    let completed_vault = test_support::deserialize_account::<redemption_program::RedemptionVault>(
        &svm,
        &redemption_vault,
    );
    let action_log = test_support::deserialize_account::<redemption_program::AdminActionLog>(
        &svm,
        &admin_action_log,
    );

    assert_eq!(
        completed_request.status,
        redemption_program::RedemptionStatus::Completed
    );
    assert_eq!(completed_vault.outstanding_amount, 0);
    assert_eq!(completed_vault.total_completed, REDEMPTION_AMOUNT);
    assert_eq!(
        action_log.action,
        redemption_program::AdminAction::CompleteRedemption
    );
    assert_eq!(action_log.admin, admin.pubkey());
    assert_eq!(action_log.target, redemption_request);
    assert_eq!(action_log.amount, REDEMPTION_AMOUNT);
}
