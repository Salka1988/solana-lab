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
    test_support::new_svm_with_program(
        modular_issuer::id(),
        include_bytes!("../../../target/deploy/modular_issuer.so"),
    )
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
) -> Instruction {
    Instruction::new_with_bytes(
        modular_issuer::id(),
        &modular_issuer::instruction::RegisterIssuer {
            mint_limit: ISSUER_MINT_LIMIT,
        }
        .data(),
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

fn rotate_issuer_authority_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    current_issuer_config: Pubkey,
    current_issuer_stats: Pubkey,
    new_issuer_config: Pubkey,
    new_issuer_stats: Pubkey,
    current_authority: Pubkey,
    new_authority: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        modular_issuer::id(),
        &modular_issuer::instruction::RotateIssuerAuthority {}.data(),
        modular_issuer::accounts::RotateIssuerAuthority {
            admin,
            protocol_config,
            current_issuer_config,
            current_issuer_stats,
            new_issuer_config,
            new_issuer_stats,
            current_authority,
            new_authority,
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

fn register_issuer(
    svm: &mut LiteSVM,
    admin: &Keypair,
    protocol_config: Pubkey,
    issuer_authority: Pubkey,
) -> (Pubkey, Pubkey) {
    let (issuer_config, _) = issuer_config_pda(issuer_authority);
    let (issuer_stats, _) = issuer_stats_pda(issuer_authority);

    assert!(test_support::send_instruction(
        svm,
        admin,
        register_issuer_ix(
            admin.pubkey(),
            protocol_config,
            issuer_config,
            issuer_stats,
            issuer_authority,
        ),
    ));

    (issuer_config, issuer_stats)
}

#[test]
fn rotate_issuer_authority_creates_new_issuer_accounts_and_retires_old_config() {
    let (mut svm, admin) = setup();
    let current_authority = Keypair::new().pubkey();
    let new_authority = Keypair::new().pubkey();
    let (protocol_config, _) = protocol_config_pda();

    initialize_protocol(&mut svm, &admin, protocol_config);
    let (current_issuer_config, current_issuer_stats) =
        register_issuer(&mut svm, &admin, protocol_config, current_authority);
    let (new_issuer_config, expected_new_config_bump) = issuer_config_pda(new_authority);
    let (new_issuer_stats, expected_new_stats_bump) = issuer_stats_pda(new_authority);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        rotate_issuer_authority_ix(
            admin.pubkey(),
            protocol_config,
            current_issuer_config,
            current_issuer_stats,
            new_issuer_config,
            new_issuer_stats,
            current_authority,
            new_authority,
        ),
    ));

    let old_config = test_support::deserialize_account::<modular_issuer::IssuerConfig>(
        &svm,
        &current_issuer_config,
    );
    let new_config =
        test_support::deserialize_account::<modular_issuer::IssuerConfig>(&svm, &new_issuer_config);
    let new_stats =
        test_support::deserialize_account::<modular_issuer::IssuerStats>(&svm, &new_issuer_stats);

    assert_eq!(old_config.authority, current_authority);
    assert_eq!(old_config.mint_limit, 0);
    assert!(old_config.paused);
    assert_eq!(new_config.protocol_config, protocol_config);
    assert_eq!(new_config.authority, new_authority);
    assert_eq!(new_config.stats, new_issuer_stats);
    assert_eq!(new_config.mint_limit, ISSUER_MINT_LIMIT);
    assert!(!new_config.paused);
    assert_eq!(new_config.bump, expected_new_config_bump);
    assert_eq!(new_stats.protocol_config, protocol_config);
    assert_eq!(new_stats.issuer_config, new_issuer_config);
    assert_eq!(new_stats.authority, new_authority);
    assert_eq!(new_stats.current_outstanding, 0);
    assert_eq!(new_stats.total_minted, 0);
    assert_eq!(new_stats.total_burned, 0);
    assert_eq!(new_stats.bump, expected_new_stats_bump);
}

#[test]
fn rotate_issuer_authority_rejects_non_admin() {
    let (mut svm, admin) = setup();
    let attacker = Keypair::new();
    let current_authority = Keypair::new().pubkey();
    let new_authority = Keypair::new().pubkey();
    let (protocol_config, _) = protocol_config_pda();

    test_support::fund_user(&mut svm, &attacker);
    initialize_protocol(&mut svm, &admin, protocol_config);
    let (current_issuer_config, current_issuer_stats) =
        register_issuer(&mut svm, &admin, protocol_config, current_authority);
    let (new_issuer_config, _) = issuer_config_pda(new_authority);
    let (new_issuer_stats, _) = issuer_stats_pda(new_authority);

    let result = test_support::send_instruction_result(
        &mut svm,
        &attacker,
        rotate_issuer_authority_ix(
            attacker.pubkey(),
            protocol_config,
            current_issuer_config,
            current_issuer_stats,
            new_issuer_config,
            new_issuer_stats,
            current_authority,
            new_authority,
        ),
    );

    test_support::assert_result_fails_with(result, "ConstraintHasOne");
}

#[test]
fn rotate_issuer_authority_rejects_same_authority() {
    let (mut svm, admin) = setup();
    let current_authority = Keypair::new().pubkey();
    let (protocol_config, _) = protocol_config_pda();

    initialize_protocol(&mut svm, &admin, protocol_config);
    let (current_issuer_config, current_issuer_stats) =
        register_issuer(&mut svm, &admin, protocol_config, current_authority);

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        rotate_issuer_authority_ix(
            admin.pubkey(),
            protocol_config,
            current_issuer_config,
            current_issuer_stats,
            current_issuer_config,
            current_issuer_stats,
            current_authority,
            current_authority,
        ),
    );

    test_support::assert_result_fails_with(result, "already in use");
}

#[test]
fn rotate_issuer_authority_rejects_wrong_current_issuer_config_pda() {
    let (mut svm, admin) = setup();
    let current_authority = Keypair::new().pubkey();
    let other_authority = Keypair::new().pubkey();
    let new_authority = Keypair::new().pubkey();
    let (protocol_config, _) = protocol_config_pda();

    initialize_protocol(&mut svm, &admin, protocol_config);
    let (_, current_issuer_stats) =
        register_issuer(&mut svm, &admin, protocol_config, current_authority);
    let (wrong_issuer_config, _) =
        register_issuer(&mut svm, &admin, protocol_config, other_authority);
    let (new_issuer_config, _) = issuer_config_pda(new_authority);
    let (new_issuer_stats, _) = issuer_stats_pda(new_authority);

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        rotate_issuer_authority_ix(
            admin.pubkey(),
            protocol_config,
            wrong_issuer_config,
            current_issuer_stats,
            new_issuer_config,
            new_issuer_stats,
            current_authority,
            new_authority,
        ),
    );

    test_support::assert_result_fails_with(result, "ConstraintSeeds");
}

#[test]
fn rotate_issuer_authority_rejects_duplicate_new_authority() {
    let (mut svm, admin) = setup();
    let current_authority = Keypair::new().pubkey();
    let existing_authority = Keypair::new().pubkey();
    let (protocol_config, _) = protocol_config_pda();

    initialize_protocol(&mut svm, &admin, protocol_config);
    let (current_issuer_config, current_issuer_stats) =
        register_issuer(&mut svm, &admin, protocol_config, current_authority);
    let (existing_issuer_config, existing_issuer_stats) =
        register_issuer(&mut svm, &admin, protocol_config, existing_authority);

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        rotate_issuer_authority_ix(
            admin.pubkey(),
            protocol_config,
            current_issuer_config,
            current_issuer_stats,
            existing_issuer_config,
            existing_issuer_stats,
            current_authority,
            existing_authority,
        ),
    );

    test_support::assert_result_fails_with(result, "already in use");
}
