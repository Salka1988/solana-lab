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
const DEFAULT_LIMIT_LAMPORTS: u64 = 500_000_000;

fn setup() -> (LiteSVM, Keypair) {
    let (mut svm, payer) = test_support::new_svm_with_payer();
    let bytes = include_bytes!("../../../target/deploy/vault_authority.so");

    test_support::add_program(&mut svm, vault_authority::id(), bytes);

    (svm, payer)
}

fn vault_config_pda(user: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[vault_authority::VAULT_SEED, user.as_ref()],
        &vault_authority::id(),
    )
}

fn initialize_vault_ix(user: Pubkey, vault_config: Pubkey, limit_lamports: u64) -> Instruction {
    Instruction::new_with_bytes(
        vault_authority::id(),
        &vault_authority::instruction::InitializeVault { limit_lamports }.data(),
        vault_authority::accounts::InitializeVault {
            user,
            vault_config,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn send_instruction(svm: &mut LiteSVM, signer: &Keypair, instruction: Instruction) -> bool {
    test_support::send_instruction(svm, signer, instruction)
}

#[test]
fn initialize_vault_creates_expected_pda_state() {
    let (mut svm, user) = setup();
    let (vault_config, expected_bump) = vault_config_pda(&user.pubkey());
    let instruction = initialize_vault_ix(user.pubkey(), vault_config, DEFAULT_LIMIT_LAMPORTS);

    assert!(send_instruction(&mut svm, &user, instruction));

    let vault_state =
        test_support::deserialize_account::<vault_authority::VaultConfig>(&svm, &vault_config);

    assert_eq!(vault_state.user, user.pubkey());
    assert_eq!(vault_state.bump, expected_bump);
    assert_eq!(vault_state.limit_lamports, DEFAULT_LIMIT_LAMPORTS);
    assert_eq!(vault_state.total_deposited_lamports, 0);
}

#[test]
fn initialize_vault_fails_for_duplicate_pda() {
    let (mut svm, user) = setup();
    let (vault_config, _) = vault_config_pda(&user.pubkey());

    assert!(send_instruction(
        &mut svm,
        &user,
        initialize_vault_ix(user.pubkey(), vault_config, DEFAULT_LIMIT_LAMPORTS)
    ));

    assert!(!send_instruction(
        &mut svm,
        &user,
        initialize_vault_ix(user.pubkey(), vault_config, DEFAULT_LIMIT_LAMPORTS)
    ));
}
