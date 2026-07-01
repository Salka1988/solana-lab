use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{
            instruction::{AccountMeta, Instruction},
            system_program,
        },
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
    spl_transfer_hook_interface::instruction::TransferHookInstruction,
};

const MAX_TRANSFER_AMOUNT: u64 = 1_000;
const DAILY_TRANSFER_LIMIT: u64 = 2_000;

fn setup() -> (LiteSVM, Keypair) {
    let (mut svm, payer) = test_support::new_svm_with_payer();
    let bytes = include_bytes!("../../../target/deploy/compliance_hook.so");

    test_support::add_program(&mut svm, compliance_hook::id(), bytes);

    (svm, payer)
}

fn compliance_config_pda(mint: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[compliance_hook::COMPLIANCE_CONFIG_SEED, mint.as_ref()],
        &compliance_hook::id(),
    )
}

fn user_compliance_pda(mint: Pubkey, user: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            compliance_hook::USER_COMPLIANCE_SEED,
            mint.as_ref(),
            user.as_ref(),
        ],
        &compliance_hook::id(),
    )
}

fn extra_account_meta_list_pda(mint: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[compliance_hook::EXTRA_ACCOUNT_METAS_SEED, mint.as_ref()],
        &compliance_hook::id(),
    )
}

fn initialize_compliance_config_ix(
    admin: Pubkey,
    config: Pubkey,
    mint: Pubkey,
    max_transfer_amount: u64,
    daily_transfer_limit: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        compliance_hook::id(),
        &compliance_hook::instruction::InitializeComplianceConfig {
            max_transfer_amount,
            daily_transfer_limit,
        }
        .data(),
        compliance_hook::accounts::InitializeComplianceConfig {
            admin,
            config,
            mint,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn initialize_extra_account_meta_list_ix(
    admin: Pubkey,
    config: Pubkey,
    extra_account_meta_list: Pubkey,
    mint: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        compliance_hook::id(),
        &compliance_hook::instruction::InitializeExtraAccountMetaList {}.data(),
        compliance_hook::accounts::InitializeExtraAccountMetaList {
            admin,
            config,
            extra_account_meta_list,
            mint,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn set_user_compliance_ix(
    admin: Pubkey,
    config: Pubkey,
    user_compliance: Pubkey,
    user: Pubkey,
    mint: Pubkey,
    allowlisted: bool,
    blocked: bool,
    issuer_active: bool,
) -> Instruction {
    Instruction::new_with_bytes(
        compliance_hook::id(),
        &compliance_hook::instruction::SetUserCompliance {
            allowlisted,
            blocked,
            issuer_active,
        }
        .data(),
        compliance_hook::accounts::SetUserCompliance {
            admin,
            config,
            user_compliance,
            user,
            mint,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn set_transfer_limits_ix(
    admin: Pubkey,
    config: Pubkey,
    max_transfer_amount: u64,
    daily_transfer_limit: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        compliance_hook::id(),
        &compliance_hook::instruction::SetTransferLimits {
            max_transfer_amount,
            daily_transfer_limit,
        }
        .data(),
        compliance_hook::accounts::SetTransferLimits { admin, config }.to_account_metas(None),
    )
}

fn set_protocol_paused_ix(admin: Pubkey, config: Pubkey, paused: bool) -> Instruction {
    Instruction::new_with_bytes(
        compliance_hook::id(),
        &compliance_hook::instruction::SetProtocolPaused { paused }.data(),
        compliance_hook::accounts::SetProtocolPaused { admin, config }.to_account_metas(None),
    )
}

fn execute_hook_ix(
    source: Pubkey,
    mint: Pubkey,
    destination: Pubkey,
    authority: Pubkey,
    config: Pubkey,
    source_compliance: Pubkey,
    destination_compliance: Pubkey,
    amount: u64,
) -> Instruction {
    let (extra_account_meta_list, _) = extra_account_meta_list_pda(mint);

    Instruction {
        program_id: compliance_hook::id(),
        accounts: vec![
            AccountMeta::new_readonly(source, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(destination, false),
            AccountMeta::new_readonly(authority, false),
            AccountMeta::new_readonly(extra_account_meta_list, false),
            AccountMeta::new_readonly(config, false),
            AccountMeta::new(source_compliance, false),
            AccountMeta::new_readonly(destination_compliance, false),
        ],
        data: TransferHookInstruction::Execute { amount }.pack(),
    }
}

struct Fixture {
    mint: Pubkey,
    source: Pubkey,
    destination: Pubkey,
    authority: Pubkey,
    config: Pubkey,
    source_compliance: Pubkey,
    destination_compliance: Pubkey,
}

fn initialize_fixture(svm: &mut LiteSVM, admin: &Keypair) -> Fixture {
    let mint = Keypair::new().pubkey();
    let source = Keypair::new().pubkey();
    let destination = Keypair::new().pubkey();
    let authority = Keypair::new().pubkey();
    let (config, _) = compliance_config_pda(mint);
    let (source_compliance, _) = user_compliance_pda(mint, source);
    let (destination_compliance, _) = user_compliance_pda(mint, destination);
    let (extra_account_meta_list, _) = extra_account_meta_list_pda(mint);

    assert!(test_support::send_instruction(
        svm,
        admin,
        initialize_compliance_config_ix(
            admin.pubkey(),
            config,
            mint,
            MAX_TRANSFER_AMOUNT,
            DAILY_TRANSFER_LIMIT,
        ),
    ));

    assert!(test_support::send_instruction(
        svm,
        admin,
        initialize_extra_account_meta_list_ix(
            admin.pubkey(),
            config,
            extra_account_meta_list,
            mint,
        ),
    ));

    assert!(test_support::send_instruction(
        svm,
        admin,
        set_user_compliance_ix(
            admin.pubkey(),
            config,
            source_compliance,
            source,
            mint,
            true,
            false,
            true,
        ),
    ));

    assert!(test_support::send_instruction(
        svm,
        admin,
        set_user_compliance_ix(
            admin.pubkey(),
            config,
            destination_compliance,
            destination,
            mint,
            true,
            false,
            true,
        ),
    ));

    Fixture {
        mint,
        source,
        destination,
        authority,
        config,
        source_compliance,
        destination_compliance,
    }
}

#[test]
fn initialize_compliance_config_creates_expected_state() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new().pubkey();
    let (config, expected_bump) = compliance_config_pda(mint);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_compliance_config_ix(
            admin.pubkey(),
            config,
            mint,
            MAX_TRANSFER_AMOUNT,
            DAILY_TRANSFER_LIMIT,
        ),
    ));

    let state =
        test_support::deserialize_account::<compliance_hook::ComplianceConfig>(&svm, &config);

    assert_eq!(state.admin, admin.pubkey());
    assert_eq!(state.mint, mint);
    assert_eq!(state.max_transfer_amount, MAX_TRANSFER_AMOUNT);
    assert_eq!(state.daily_transfer_limit, DAILY_TRANSFER_LIMIT);
    assert!(!state.paused);
    assert_eq!(state.bump, expected_bump);
}

#[test]
fn initialize_extra_account_meta_list_creates_validation_account() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new().pubkey();
    let (config, _) = compliance_config_pda(mint);
    let (extra_account_meta_list, _) = extra_account_meta_list_pda(mint);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_compliance_config_ix(
            admin.pubkey(),
            config,
            mint,
            MAX_TRANSFER_AMOUNT,
            DAILY_TRANSFER_LIMIT,
        ),
    ));
    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_extra_account_meta_list_ix(
            admin.pubkey(),
            config,
            extra_account_meta_list,
            mint,
        ),
    ));

    let account = svm
        .get_account(&extra_account_meta_list)
        .expect("validation account exists");

    assert_eq!(account.owner, compliance_hook::id());
    assert_eq!(
        account.data.len(),
        compliance_hook::EXTRA_ACCOUNT_META_LIST_SPACE
    );
}

#[test]
fn execute_allows_allowlisted_transfer_and_updates_daily_amount() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        execute_hook_ix(
            fixture.source,
            fixture.mint,
            fixture.destination,
            fixture.authority,
            fixture.config,
            fixture.source_compliance,
            fixture.destination_compliance,
            500,
        ),
    ));

    let source_state = test_support::deserialize_account::<compliance_hook::UserCompliance>(
        &svm,
        &fixture.source_compliance,
    );

    assert_eq!(source_state.transferred_today, 500);
}

#[test]
fn execute_rejects_blocked_source() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        set_user_compliance_ix(
            admin.pubkey(),
            fixture.config,
            fixture.source_compliance,
            fixture.source,
            fixture.mint,
            true,
            true,
            true,
        ),
    ));

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        execute_hook_ix(
            fixture.source,
            fixture.mint,
            fixture.destination,
            fixture.authority,
            fixture.config,
            fixture.source_compliance,
            fixture.destination_compliance,
            500,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "SourceBlocked");
}

#[test]
fn execute_rejects_non_allowlisted_destination() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        set_user_compliance_ix(
            admin.pubkey(),
            fixture.config,
            fixture.destination_compliance,
            fixture.destination,
            fixture.mint,
            false,
            false,
            true,
        ),
    ));

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        execute_hook_ix(
            fixture.source,
            fixture.mint,
            fixture.destination,
            fixture.authority,
            fixture.config,
            fixture.source_compliance,
            fixture.destination_compliance,
            500,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "DestinationNotAllowlisted");
}

#[test]
fn execute_rejects_per_transfer_limit() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        execute_hook_ix(
            fixture.source,
            fixture.mint,
            fixture.destination,
            fixture.authority,
            fixture.config,
            fixture.source_compliance,
            fixture.destination_compliance,
            MAX_TRANSFER_AMOUNT + 1,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "TransferLimitExceeded");
}

#[test]
fn execute_rejects_daily_limit() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        execute_hook_ix(
            fixture.source,
            fixture.mint,
            fixture.destination,
            fixture.authority,
            fixture.config,
            fixture.source_compliance,
            fixture.destination_compliance,
            1_000,
        ),
    ));

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        set_transfer_limits_ix(admin.pubkey(), fixture.config, 2_000, 1_500),
    ));

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        execute_hook_ix(
            fixture.source,
            fixture.mint,
            fixture.destination,
            fixture.authority,
            fixture.config,
            fixture.source_compliance,
            fixture.destination_compliance,
            600,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "DailyLimitExceeded");
}

#[test]
fn execute_rejects_paused_protocol() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        set_protocol_paused_ix(admin.pubkey(), fixture.config, true),
    ));

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        execute_hook_ix(
            fixture.source,
            fixture.mint,
            fixture.destination,
            fixture.authority,
            fixture.config,
            fixture.source_compliance,
            fixture.destination_compliance,
            500,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "ProtocolPaused");
}

#[test]
fn execute_rejects_inactive_source_issuer() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        set_user_compliance_ix(
            admin.pubkey(),
            fixture.config,
            fixture.source_compliance,
            fixture.source,
            fixture.mint,
            true,
            false,
            false,
        ),
    ));

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        execute_hook_ix(
            fixture.source,
            fixture.mint,
            fixture.destination,
            fixture.authority,
            fixture.config,
            fixture.source_compliance,
            fixture.destination_compliance,
            500,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "IssuerInactive");
}

#[test]
fn execute_rejects_wrong_extra_account_order() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        execute_hook_ix(
            fixture.source,
            fixture.mint,
            fixture.destination,
            fixture.authority,
            fixture.config,
            fixture.destination_compliance,
            fixture.source_compliance,
            500,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "InvalidTransferHookAccounts");
}
