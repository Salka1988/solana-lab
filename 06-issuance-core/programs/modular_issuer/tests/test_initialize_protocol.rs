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

fn setup() -> (LiteSVM, Keypair) {
    test_support::new_svm_with_program(
        modular_issuer::id(),
        include_bytes!("../../../target/deploy/modular_issuer.so"),
    )
}

fn protocol_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[modular_issuer::PROTOCOL_SEED], &modular_issuer::id())
}

fn initialize_protocol_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    global_supply_cap: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        modular_issuer::id(),
        &modular_issuer::instruction::InitializeProtocol { global_supply_cap }.data(),
        modular_issuer::accounts::InitializeProtocol {
            admin,
            protocol_config,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

#[test]
fn initialize_protocol_creates_expected_config() {
    let (mut svm, admin) = setup();
    let (protocol_config, expected_bump) = protocol_config_pda();
    let instruction = initialize_protocol_ix(admin.pubkey(), protocol_config, GLOBAL_SUPPLY_CAP);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        instruction
    ));

    let state =
        test_support::deserialize_account::<modular_issuer::ProtocolConfig>(&svm, &protocol_config);

    assert_eq!(state.admin, admin.pubkey());
    assert_eq!(state.pending_admin, Pubkey::default());
    assert_eq!(state.stablecoin_mint, Pubkey::default());
    assert_eq!(state.global_supply_cap, GLOBAL_SUPPLY_CAP);
    assert!(!state.paused);
    assert_eq!(state.bump, expected_bump);
}

#[test]
fn initialize_protocol_rejects_duplicate_config() {
    let (mut svm, admin) = setup();
    let (protocol_config, _) = protocol_config_pda();

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config, GLOBAL_SUPPLY_CAP)
    ));

    assert!(!test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config, GLOBAL_SUPPLY_CAP)
    ));
}

#[test]
fn initialize_protocol_rejects_zero_global_supply_cap() {
    let (mut svm, admin) = setup();
    let (protocol_config, _) = protocol_config_pda();

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config, 0),
    );

    test_support::assert_result_fails_with(result, "InvalidGlobalSupplyCap");
}

#[test]
fn initialize_protocol_rejects_wrong_pda_substitution() {
    let (mut svm, admin) = setup();
    let wrong_protocol_config = Keypair::new().pubkey();

    let result = test_support::send_instruction_result(
        &mut svm,
        &admin,
        initialize_protocol_ix(admin.pubkey(), wrong_protocol_config, GLOBAL_SUPPLY_CAP),
    );

    test_support::assert_result_fails_with(result, "ConstraintSeeds");
}
