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
const DEPOSIT_LAMPORTS: u64 = 125_000_000;

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

fn deposit_ix(user: Pubkey, vault_config: Pubkey, amount_lamports: u64) -> Instruction {
    Instruction::new_with_bytes(
        vault_authority::id(),
        &vault_authority::instruction::Deposit { amount_lamports }.data(),
        vault_authority::accounts::Deposit {
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

fn read_vault_config(svm: &LiteSVM, vault_config: &Pubkey) -> vault_authority::VaultConfig {
    test_support::deserialize_account(svm, vault_config)
}

#[test]
fn deposit_updates_state_and_vault_lamports() {
    let (mut svm, user) = setup();
    let (vault_config, _) = vault_config_pda(&user.pubkey());

    assert!(send_instruction(
        &mut svm,
        &user,
        initialize_vault_ix(user.pubkey(), vault_config, DEFAULT_LIMIT_LAMPORTS)
    ));

    let vault_lamports_before = svm.get_account(&vault_config).unwrap().lamports;

    assert!(send_instruction(
        &mut svm,
        &user,
        deposit_ix(user.pubkey(), vault_config, DEPOSIT_LAMPORTS)
    ));

    let vault_state = read_vault_config(&svm, &vault_config);
    let vault_lamports_after = svm.get_account(&vault_config).unwrap().lamports;

    assert_eq!(vault_state.total_deposited_lamports, DEPOSIT_LAMPORTS);
    assert_eq!(
        vault_lamports_after,
        vault_lamports_before + DEPOSIT_LAMPORTS
    );
}

#[test]
fn deposit_rejects_amount_over_limit() {
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
        deposit_ix(user.pubkey(), vault_config, DEFAULT_LIMIT_LAMPORTS + 1)
    ));

    let vault_state = read_vault_config(&svm, &vault_config);

    assert_eq!(vault_state.total_deposited_lamports, 0);
}

#[test]
fn deposit_rejects_wrong_pda_substitution() {
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
        deposit_ix(user.pubkey(), other_vault_config, DEPOSIT_LAMPORTS)
    ));

    let vault_state = read_vault_config(&svm, &vault_config);
    let other_vault_state = read_vault_config(&svm, &other_vault_config);

    assert_eq!(vault_state.total_deposited_lamports, 0);
    assert_eq!(other_vault_state.total_deposited_lamports, 0);
}
