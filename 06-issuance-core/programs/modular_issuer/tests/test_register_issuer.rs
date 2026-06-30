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

const GLOBAL_SUPPLY_CAP: u64 = 1_000_000_000_000;
const ISSUER_MINT_LIMIT: u64 = 100_000_000;

fn setup() -> (LiteSVM, Keypair) {
    let (mut svm, payer) = test_support::new_svm_with_payer();
    let bytes = include_bytes!("../../../target/deploy/modular_issuer.so");

    test_support::add_program(&mut svm, modular_issuer::id(), bytes);

    (svm, payer)
}

fn protocol_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[modular_issuer::PROTOCOL_SEED], &modular_issuer::id())
}

fn issuer_config_pda(issuer_authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[modular_issuer::ISSUER_SEED, issuer_authority.as_ref()],
        &modular_issuer::id(),
    )
}

fn issuer_stats_pda(issuer_authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[modular_issuer::ISSUER_STATS_SEED, issuer_authority.as_ref()],
        &modular_issuer::id(),
    )
}

fn initialize_protocol_ix(admin: Pubkey, protocol_config: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        modular_issuer::id(),
        &modular_issuer::instruction::InitializeProtocol {
            global_supply_cap: GLOBAL_SUPPLY_CAP,
        }
        .data(),
        modular_issuer::accounts::InitializeProtocol {
            admin,
            protocol_config,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn register_issuer_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    issuer_config: Pubkey,
    issuer_stats: Pubkey,
    issuer_authority: Pubkey,
    mint_limit: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        modular_issuer::id(),
        &modular_issuer::instruction::RegisterIssuer { mint_limit }.data(),
        modular_issuer::accounts::RegisterIssuer {
            admin,
            protocol_config,
            issuer_config,
            issuer_stats,
            issuer_authority,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn initialize_protocol(svm: &mut LiteSVM, admin: &Keypair, protocol_config: Pubkey) {
    assert!(test_support::send_instruction(
        svm,
        admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config)
    ));
}

#[test]
fn register_issuer_initializes_config_and_stats() {
    let (mut svm, admin) = setup();
    let issuer_authority = Keypair::new().pubkey();
    let (protocol_config, _) = protocol_config_pda();
    let (issuer_config, expected_issuer_config_bump) = issuer_config_pda(issuer_authority);
    let (issuer_stats, expected_issuer_stats_bump) = issuer_stats_pda(issuer_authority);

    initialize_protocol(&mut svm, &admin, protocol_config);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        register_issuer_ix(
            admin.pubkey(),
            protocol_config,
            issuer_config,
            issuer_stats,
            issuer_authority,
            ISSUER_MINT_LIMIT,
        ),
    ));

    let issuer_config_state =
        test_support::deserialize_account::<modular_issuer::IssuerConfig>(&svm, &issuer_config);
    let issuer_stats_state =
        test_support::deserialize_account::<modular_issuer::IssuerStats>(&svm, &issuer_stats);

    assert_eq!(issuer_config_state.protocol_config, protocol_config);
    assert_eq!(issuer_config_state.authority, issuer_authority);
    assert_eq!(issuer_config_state.stats, issuer_stats);
    assert_eq!(issuer_config_state.mint_limit, ISSUER_MINT_LIMIT);
    assert!(!issuer_config_state.paused);
    assert_eq!(issuer_config_state.bump, expected_issuer_config_bump);
    assert_eq!(issuer_stats_state.protocol_config, protocol_config);
    assert_eq!(issuer_stats_state.issuer_config, issuer_config);
    assert_eq!(issuer_stats_state.authority, issuer_authority);
    assert_eq!(issuer_stats_state.current_outstanding, 0);
    assert_eq!(issuer_stats_state.total_minted, 0);
    assert_eq!(issuer_stats_state.total_burned, 0);
    assert_eq!(issuer_stats_state.bump, expected_issuer_stats_bump);
}

#[test]
fn register_issuer_rejects_zero_mint_limit() {
    let (mut svm, admin) = setup();
    let issuer_authority = Keypair::new().pubkey();
    let (protocol_config, _) = protocol_config_pda();
    let (issuer_config, _) = issuer_config_pda(issuer_authority);
    let (issuer_stats, _) = issuer_stats_pda(issuer_authority);

    initialize_protocol(&mut svm, &admin, protocol_config);

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        register_issuer_ix(
            admin.pubkey(),
            protocol_config,
            issuer_config,
            issuer_stats,
            issuer_authority,
            0,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "InvalidIssuerMintLimit");
}

#[test]
fn register_issuer_rejects_non_admin() {
    let (mut svm, admin) = setup();
    let attacker = Keypair::new();
    let issuer_authority = Keypair::new().pubkey();
    let (protocol_config, _) = protocol_config_pda();
    let (issuer_config, _) = issuer_config_pda(issuer_authority);
    let (issuer_stats, _) = issuer_stats_pda(issuer_authority);

    test_support::fund_user(&mut svm, &attacker);
    initialize_protocol(&mut svm, &admin, protocol_config);

    let result = test_support::send_instruction_result(
        &mut svm,
        &attacker,
        register_issuer_ix(
            attacker.pubkey(),
            protocol_config,
            issuer_config,
            issuer_stats,
            issuer_authority,
            ISSUER_MINT_LIMIT + 1,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "ConstraintHasOne");
}

#[test]
fn register_issuer_rejects_wrong_issuer_config_pda() {
    let (mut svm, admin) = setup();
    let issuer_authority = Keypair::new().pubkey();
    let (protocol_config, _) = protocol_config_pda();
    let wrong_issuer_config = Keypair::new().pubkey();
    let (issuer_stats, _) = issuer_stats_pda(issuer_authority);

    initialize_protocol(&mut svm, &admin, protocol_config);

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        register_issuer_ix(
            admin.pubkey(),
            protocol_config,
            wrong_issuer_config,
            issuer_stats,
            issuer_authority,
            ISSUER_MINT_LIMIT,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "ConstraintSeeds");
}

#[test]
fn register_issuer_rejects_wrong_issuer_stats_pda() {
    let (mut svm, admin) = setup();
    let issuer_authority = Keypair::new().pubkey();
    let (protocol_config, _) = protocol_config_pda();
    let (issuer_config, _) = issuer_config_pda(issuer_authority);
    let wrong_issuer_stats = Keypair::new().pubkey();

    initialize_protocol(&mut svm, &admin, protocol_config);

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        register_issuer_ix(
            admin.pubkey(),
            protocol_config,
            issuer_config,
            wrong_issuer_stats,
            issuer_authority,
            ISSUER_MINT_LIMIT,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "ConstraintSeeds");
}

#[test]
fn register_issuer_rejects_duplicate_issuer() {
    let (mut svm, admin) = setup();
    let issuer_authority = Keypair::new().pubkey();
    let (protocol_config, _) = protocol_config_pda();
    let (issuer_config, _) = issuer_config_pda(issuer_authority);
    let (issuer_stats, _) = issuer_stats_pda(issuer_authority);

    initialize_protocol(&mut svm, &admin, protocol_config);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        register_issuer_ix(
            admin.pubkey(),
            protocol_config,
            issuer_config,
            issuer_stats,
            issuer_authority,
            ISSUER_MINT_LIMIT,
        ),
    ));

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        register_issuer_ix(
            admin.pubkey(),
            protocol_config,
            issuer_config,
            issuer_stats,
            issuer_authority,
            ISSUER_MINT_LIMIT + 1,
        ),
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "already in use");
}
