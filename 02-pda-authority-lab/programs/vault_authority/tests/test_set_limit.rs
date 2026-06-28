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
const UPDATED_LIMIT_LAMPORTS: u64 = 750_000_000;

fn setup() -> (LiteSVM, Keypair) {
    let (mut svm, payer) = test_support::new_svm_with_payer();
    let bytes = include_bytes!("../../../target/deploy/vault_authority.so");

    test_support::add_program(&mut svm, vault_authority::id(), bytes);

    (svm, payer)
}

fn fund_user(svm: &mut LiteSVM, user: &Keypair) {
    test_support::fund_user(svm, user);
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

fn set_vault_limit_ix(user: Pubkey, vault_config: Pubkey, limit_lamports: u64) -> Instruction {
    Instruction::new_with_bytes(
        vault_authority::id(),
        &vault_authority::instruction::SetVaultLimit { limit_lamports }.data(),
        vault_authority::accounts::SetVaultLimit { user, vault_config }.to_account_metas(None),
    )
}

fn send_instruction(svm: &mut LiteSVM, signer: &Keypair, instruction: Instruction) -> bool {
    test_support::send_instruction(svm, signer, instruction)
}

fn read_vault_config(svm: &LiteSVM, vault_config: &Pubkey) -> vault_authority::VaultConfig {
    test_support::deserialize_account(svm, vault_config)
}

#[test]
fn set_vault_limit_updates_owner_vault() {
    let (mut svm, user) = setup();
    let (vault_config, _) = vault_config_pda(&user.pubkey());

    assert!(send_instruction(
        &mut svm,
        &user,
        initialize_vault_ix(user.pubkey(), vault_config, DEFAULT_LIMIT_LAMPORTS)
    ));

    assert!(send_instruction(
        &mut svm,
        &user,
        set_vault_limit_ix(user.pubkey(), vault_config, UPDATED_LIMIT_LAMPORTS)
    ));

    let vault_state = read_vault_config(&svm, &vault_config);

    assert_eq!(vault_state.user, user.pubkey());
    assert_eq!(vault_state.limit_lamports, UPDATED_LIMIT_LAMPORTS);
    assert_eq!(vault_state.total_deposited_lamports, 0);
}

#[test]
fn set_vault_limit_rejects_unauthorized_user() {
    let (mut svm, user) = setup();
    let attacker = Keypair::new();
    let (vault_config, _) = vault_config_pda(&user.pubkey());

    fund_user(&mut svm, &attacker);

    assert!(send_instruction(
        &mut svm,
        &user,
        initialize_vault_ix(user.pubkey(), vault_config, DEFAULT_LIMIT_LAMPORTS)
    ));

    assert!(!send_instruction(
        &mut svm,
        &attacker,
        set_vault_limit_ix(attacker.pubkey(), vault_config, UPDATED_LIMIT_LAMPORTS)
    ));

    let vault_state = read_vault_config(&svm, &vault_config);

    assert_eq!(vault_state.limit_lamports, DEFAULT_LIMIT_LAMPORTS);
}

#[test]
fn set_vault_limit_rejects_wrong_pda_substitution() {
    let (mut svm, user) = setup();
    let other_user = Keypair::new();
    let (vault_config, _) = vault_config_pda(&user.pubkey());
    let (other_vault_config, _) = vault_config_pda(&other_user.pubkey());

    fund_user(&mut svm, &other_user);

    assert!(send_instruction(
        &mut svm,
        &user,
        initialize_vault_ix(user.pubkey(), vault_config, DEFAULT_LIMIT_LAMPORTS)
    ));
    assert!(send_instruction(
        &mut svm,
        &other_user,
        initialize_vault_ix(
            other_user.pubkey(),
            other_vault_config,
            DEFAULT_LIMIT_LAMPORTS
        )
    ));

    assert!(!send_instruction(
        &mut svm,
        &user,
        set_vault_limit_ix(user.pubkey(), other_vault_config, UPDATED_LIMIT_LAMPORTS)
    ));

    let vault_state = read_vault_config(&svm, &vault_config);
    let other_vault_state = read_vault_config(&svm, &other_vault_config);

    assert_eq!(vault_state.limit_lamports, DEFAULT_LIMIT_LAMPORTS);
    assert_eq!(other_vault_state.limit_lamports, DEFAULT_LIMIT_LAMPORTS);
}
