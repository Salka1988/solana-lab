use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{
            instruction::{AccountMeta, Instruction},
            system_instruction, system_program,
        },
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::{
        spl_token_2022::{
            extension::{
                transfer_hook::instruction as transfer_hook_extension_instruction, ExtensionType,
            },
            instruction as token_2022_instruction,
            state::{Account as TokenAccount, Mint},
        },
        ID as TOKEN_2022_PROGRAM_ID,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
};
use {compliance_hook as hook_program, solana_m0_test_support as test_support};

const DECIMALS: u8 = 6;
const INITIAL_BALANCE: u64 = 10_000;
const MAX_TRANSFER_AMOUNT: u64 = 1_000;
const DAILY_TRANSFER_LIMIT: u64 = 2_000;

fn setup() -> (LiteSVM, Keypair) {
    test_support::new_svm_with_program(
        hook_program::id(),
        include_bytes!("../../07-transfer-hook-compliance/target/deploy/compliance_hook.so"),
    )
}

fn compliance_config_pda(mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[hook_program::COMPLIANCE_CONFIG_SEED, mint.as_ref()],
        &hook_program::id(),
    )
    .0
}

fn user_compliance_pda(mint: Pubkey, user: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            hook_program::USER_COMPLIANCE_SEED,
            mint.as_ref(),
            user.as_ref(),
        ],
        &hook_program::id(),
    )
    .0
}

fn extra_account_meta_list_pda(mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[hook_program::EXTRA_ACCOUNT_METAS_SEED, mint.as_ref()],
        &hook_program::id(),
    )
    .0
}

fn initialize_compliance_config_ix(
    admin: Pubkey,
    config: Pubkey,
    mint: Pubkey,
    max_transfer_amount: u64,
    daily_transfer_limit: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        hook_program::id(),
        &hook_program::instruction::InitializeComplianceConfig {
            max_transfer_amount,
            daily_transfer_limit,
        }
        .data(),
        hook_program::accounts::InitializeComplianceConfig {
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
        hook_program::id(),
        &hook_program::instruction::InitializeExtraAccountMetaList {}.data(),
        hook_program::accounts::InitializeExtraAccountMetaList {
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
        hook_program::id(),
        &hook_program::instruction::SetUserCompliance {
            allowlisted,
            blocked,
            issuer_active,
        }
        .data(),
        hook_program::accounts::SetUserCompliance {
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

fn set_protocol_paused_ix(admin: Pubkey, config: Pubkey, paused: bool) -> Instruction {
    Instruction::new_with_bytes(
        hook_program::id(),
        &hook_program::instruction::SetProtocolPaused { paused }.data(),
        hook_program::accounts::SetProtocolPaused { admin, config }.to_account_metas(None),
    )
}

fn create_transfer_hook_mint_ixs(
    svm: &LiteSVM,
    payer: Pubkey,
    mint: Pubkey,
    mint_authority: Pubkey,
) -> Vec<Instruction> {
    let mint_len =
        ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::TransferHook]).unwrap();
    let rent = svm.minimum_balance_for_rent_exemption(mint_len);

    vec![
        system_instruction::create_account(
            &payer,
            &mint,
            rent,
            mint_len as u64,
            &TOKEN_2022_PROGRAM_ID,
        ),
        transfer_hook_extension_instruction::initialize(
            &TOKEN_2022_PROGRAM_ID,
            &mint,
            Some(mint_authority),
            Some(hook_program::id()),
        )
        .unwrap(),
        token_2022_instruction::initialize_mint2(
            &TOKEN_2022_PROGRAM_ID,
            &mint,
            &mint_authority,
            Some(&mint_authority),
            DECIMALS,
        )
        .unwrap(),
    ]
}

fn create_token_account_ixs(
    svm: &LiteSVM,
    payer: Pubkey,
    token_account: Pubkey,
    mint: Pubkey,
    owner: Pubkey,
) -> Vec<Instruction> {
    let account_extensions =
        ExtensionType::get_required_init_account_extensions(&[ExtensionType::TransferHook]);
    let token_account_len =
        ExtensionType::try_calculate_account_len::<TokenAccount>(&account_extensions).unwrap();
    let rent = svm.minimum_balance_for_rent_exemption(token_account_len);

    vec![
        system_instruction::create_account(
            &payer,
            &token_account,
            rent,
            token_account_len as u64,
            &TOKEN_2022_PROGRAM_ID,
        ),
        token_2022_instruction::initialize_account3(
            &TOKEN_2022_PROGRAM_ID,
            &token_account,
            &mint,
            &owner,
        )
        .unwrap(),
    ]
}

fn mint_to_ix(mint: Pubkey, token_account: Pubkey, authority: Pubkey, amount: u64) -> Instruction {
    token_2022_instruction::mint_to(
        &TOKEN_2022_PROGRAM_ID,
        &mint,
        &token_account,
        &authority,
        &[],
        amount,
    )
    .unwrap()
}

fn transfer_checked_with_hook_ix(
    source: Pubkey,
    mint: Pubkey,
    destination: Pubkey,
    owner: Pubkey,
    amount: u64,
) -> Instruction {
    let config = compliance_config_pda(mint);
    let source_compliance = user_compliance_pda(mint, source);
    let destination_compliance = user_compliance_pda(mint, destination);

    transfer_checked_with_custom_hook_accounts_ix(
        source,
        mint,
        destination,
        owner,
        config,
        source_compliance,
        destination_compliance,
        amount,
    )
}

fn transfer_checked_with_custom_hook_accounts_ix(
    source: Pubkey,
    mint: Pubkey,
    destination: Pubkey,
    owner: Pubkey,
    config: Pubkey,
    source_compliance: Pubkey,
    destination_compliance: Pubkey,
    amount: u64,
) -> Instruction {
    let extra_account_meta_list = extra_account_meta_list_pda(mint);
    let mut instruction = token_2022_instruction::transfer_checked(
        &TOKEN_2022_PROGRAM_ID,
        &source,
        &mint,
        &destination,
        &owner,
        &[],
        amount,
        DECIMALS,
    )
    .unwrap();

    instruction
        .accounts
        .push(AccountMeta::new_readonly(config, false));
    instruction
        .accounts
        .push(AccountMeta::new(source_compliance, false));
    instruction
        .accounts
        .push(AccountMeta::new_readonly(destination_compliance, false));
    instruction
        .accounts
        .push(AccountMeta::new_readonly(hook_program::id(), false));
    instruction
        .accounts
        .push(AccountMeta::new_readonly(extra_account_meta_list, false));

    instruction
}

struct Fixture {
    mint: Keypair,
    alice: Keypair,
    alice_token_account: Keypair,
    bob_token_account: Keypair,
    config: Pubkey,
    alice_compliance: Pubkey,
    bob_compliance: Pubkey,
}

fn initialize_fixture(svm: &mut LiteSVM, admin: &Keypair, daily_transfer_limit: u64) -> Fixture {
    let mint = Keypair::new();
    let alice = Keypair::new();
    let bob = Keypair::new();
    let alice_token_account = Keypair::new();
    let bob_token_account = Keypair::new();
    let config = compliance_config_pda(mint.pubkey());
    let extra_account_meta_list = extra_account_meta_list_pda(mint.pubkey());
    let alice_compliance = user_compliance_pda(mint.pubkey(), alice_token_account.pubkey());
    let bob_compliance = user_compliance_pda(mint.pubkey(), bob_token_account.pubkey());

    test_support::fund_user(svm, &alice);
    test_support::fund_user(svm, &bob);

    assert!(test_support::send_transaction(
        svm,
        admin.pubkey(),
        create_transfer_hook_mint_ixs(svm, admin.pubkey(), mint.pubkey(), admin.pubkey()),
        &[admin, &mint],
    )
    .is_ok());

    assert!(test_support::send_transaction(
        svm,
        admin.pubkey(),
        create_token_account_ixs(
            svm,
            admin.pubkey(),
            alice_token_account.pubkey(),
            mint.pubkey(),
            alice.pubkey(),
        ),
        &[admin, &alice_token_account],
    )
    .is_ok());

    assert!(test_support::send_transaction(
        svm,
        admin.pubkey(),
        create_token_account_ixs(
            svm,
            admin.pubkey(),
            bob_token_account.pubkey(),
            mint.pubkey(),
            bob.pubkey(),
        ),
        &[admin, &bob_token_account],
    )
    .is_ok());

    assert!(test_support::send_transaction(
        svm,
        admin.pubkey(),
        vec![mint_to_ix(
            mint.pubkey(),
            alice_token_account.pubkey(),
            admin.pubkey(),
            INITIAL_BALANCE,
        )],
        &[admin],
    )
    .is_ok());

    assert!(test_support::send_instruction(
        svm,
        admin,
        initialize_compliance_config_ix(
            admin.pubkey(),
            config,
            mint.pubkey(),
            MAX_TRANSFER_AMOUNT,
            daily_transfer_limit,
        ),
    ));

    assert!(test_support::send_instruction(
        svm,
        admin,
        initialize_extra_account_meta_list_ix(
            admin.pubkey(),
            config,
            extra_account_meta_list,
            mint.pubkey(),
        ),
    ));

    assert!(test_support::send_instruction(
        svm,
        admin,
        set_user_compliance_ix(
            admin.pubkey(),
            config,
            alice_compliance,
            alice_token_account.pubkey(),
            mint.pubkey(),
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
            bob_compliance,
            bob_token_account.pubkey(),
            mint.pubkey(),
            true,
            false,
            true,
        ),
    ));

    Fixture {
        mint,
        alice,
        alice_token_account,
        bob_token_account,
        config,
        alice_compliance,
        bob_compliance,
    }
}

#[test]
fn token_2022_transfer_checked_invokes_hook_and_moves_tokens() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin, DAILY_TRANSFER_LIMIT);

    assert!(test_support::send_transaction(
        &mut svm,
        admin.pubkey(),
        vec![transfer_checked_with_hook_ix(
            fixture.alice_token_account.pubkey(),
            fixture.mint.pubkey(),
            fixture.bob_token_account.pubkey(),
            fixture.alice.pubkey(),
            750,
        )],
        &[&admin, &fixture.alice],
    )
    .is_ok());

    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.alice_token_account.pubkey()),
        INITIAL_BALANCE - 750,
    );
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.bob_token_account.pubkey()),
        750
    );

    let source_state = test_support::deserialize_account::<hook_program::UserCompliance>(
        &svm,
        &fixture.alice_compliance,
    );
    assert_eq!(source_state.transferred_today, 750);
}

#[test]
fn blocked_receiver_stops_real_token_2022_transfer_and_keeps_balances() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin, DAILY_TRANSFER_LIMIT);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        set_user_compliance_ix(
            admin.pubkey(),
            fixture.config,
            fixture.bob_compliance,
            fixture.bob_token_account.pubkey(),
            fixture.mint.pubkey(),
            true,
            true,
            true,
        ),
    ));

    let result = test_support::send_transaction(
        &mut svm,
        admin.pubkey(),
        vec![transfer_checked_with_hook_ix(
            fixture.alice_token_account.pubkey(),
            fixture.mint.pubkey(),
            fixture.bob_token_account.pubkey(),
            fixture.alice.pubkey(),
            500,
        )],
        &[&admin, &fixture.alice],
    );

    test_support::assert_result_fails_with(result, "DestinationBlocked");
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.alice_token_account.pubkey()),
        INITIAL_BALANCE,
    );
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.bob_token_account.pubkey()),
        0
    );
}

#[test]
fn daily_limit_blocks_third_real_token_2022_transfer() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin, 1_500);

    for amount in [800, 700] {
        assert!(test_support::send_transaction(
            &mut svm,
            admin.pubkey(),
            vec![transfer_checked_with_hook_ix(
                fixture.alice_token_account.pubkey(),
                fixture.mint.pubkey(),
                fixture.bob_token_account.pubkey(),
                fixture.alice.pubkey(),
                amount,
            )],
            &[&admin, &fixture.alice],
        )
        .is_ok());
    }

    let result = test_support::send_transaction(
        &mut svm,
        admin.pubkey(),
        vec![transfer_checked_with_hook_ix(
            fixture.alice_token_account.pubkey(),
            fixture.mint.pubkey(),
            fixture.bob_token_account.pubkey(),
            fixture.alice.pubkey(),
            1,
        )],
        &[&admin, &fixture.alice],
    );

    test_support::assert_result_fails_with(result, "DailyLimitExceeded");
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.alice_token_account.pubkey()),
        INITIAL_BALANCE - 1_500,
    );
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.bob_token_account.pubkey()),
        1_500
    );
}

#[test]
fn paused_hook_stops_real_token_2022_transfer_and_keeps_balances() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin, DAILY_TRANSFER_LIMIT);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        set_protocol_paused_ix(admin.pubkey(), fixture.config, true),
    ));

    let result = test_support::send_transaction(
        &mut svm,
        admin.pubkey(),
        vec![transfer_checked_with_hook_ix(
            fixture.alice_token_account.pubkey(),
            fixture.mint.pubkey(),
            fixture.bob_token_account.pubkey(),
            fixture.alice.pubkey(),
            500,
        )],
        &[&admin, &fixture.alice],
    );

    test_support::assert_result_fails_with(result, "ProtocolPaused");
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.alice_token_account.pubkey()),
        INITIAL_BALANCE,
    );
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.bob_token_account.pubkey()),
        0
    );
}

#[test]
fn fake_source_compliance_account_cannot_authorize_real_transfer() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin, DAILY_TRANSFER_LIMIT);
    let fake_source = Keypair::new().pubkey();
    let fake_source_compliance = user_compliance_pda(fixture.mint.pubkey(), fake_source);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        set_user_compliance_ix(
            admin.pubkey(),
            fixture.config,
            fake_source_compliance,
            fake_source,
            fixture.mint.pubkey(),
            true,
            false,
            true,
        ),
    ));

    let result = test_support::send_transaction(
        &mut svm,
        admin.pubkey(),
        vec![transfer_checked_with_custom_hook_accounts_ix(
            fixture.alice_token_account.pubkey(),
            fixture.mint.pubkey(),
            fixture.bob_token_account.pubkey(),
            fixture.alice.pubkey(),
            fixture.config,
            fake_source_compliance,
            fixture.bob_compliance,
            500,
        )],
        &[&admin, &fixture.alice],
    );

    assert!(result.is_err());
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.alice_token_account.pubkey()),
        INITIAL_BALANCE,
    );
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.bob_token_account.pubkey()),
        0
    );
}
