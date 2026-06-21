use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const INITIAL_AIRDROP_LAMPORTS: u64 = 1_000_000_000;
const DEFAULT_LIMIT_LAMPORTS: u64 = 500_000_000;
const DEPOSIT_LAMPORTS: u64 = 125_000_000;

fn setup() -> (LiteSVM, Keypair) {
    let program_id = vault_authority::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/vault_authority.so");

    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), INITIAL_AIRDROP_LAMPORTS)
        .unwrap();

    (svm, payer)
}

fn fund_user(svm: &mut LiteSVM, user: &Keypair) {
    svm.airdrop(&user.pubkey(), INITIAL_AIRDROP_LAMPORTS)
        .unwrap();
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

fn close_vault_ix(user: Pubkey, vault_config: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        vault_authority::id(),
        &vault_authority::instruction::CloseVault {}.data(),
        vault_authority::accounts::CloseVault { user, vault_config }.to_account_metas(None),
    )
}

fn send_instruction(svm: &mut LiteSVM, signer: &Keypair, instruction: Instruction) -> bool {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();

    svm.send_transaction(tx).is_ok()
}

#[test]
fn close_vault_refunds_empty_vault_and_removes_account() {
    let (mut svm, user) = setup();
    let (vault_config, _) = vault_config_pda(&user.pubkey());

    assert!(send_instruction(
        &mut svm,
        &user,
        initialize_vault_ix(user.pubkey(), vault_config, DEFAULT_LIMIT_LAMPORTS)
    ));

    let user_lamports_before = svm.get_account(&user.pubkey()).unwrap().lamports;
    let vault_lamports_before = svm.get_account(&vault_config).unwrap().lamports;

    assert!(send_instruction(
        &mut svm,
        &user,
        close_vault_ix(user.pubkey(), vault_config)
    ));

    let user_lamports_after = svm.get_account(&user.pubkey()).unwrap().lamports;

    assert!(svm.get_account(&vault_config).is_none());
    assert!(user_lamports_after > user_lamports_before);
    assert!(user_lamports_after > user_lamports_before);
    assert!(user_lamports_after < user_lamports_before + vault_lamports_before);
}

#[test]
fn close_vault_rejects_non_empty_vault() {
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
        deposit_ix(user.pubkey(), vault_config, DEPOSIT_LAMPORTS)
    ));

    assert!(!send_instruction(
        &mut svm,
        &user,
        close_vault_ix(user.pubkey(), vault_config)
    ));

    assert!(svm.get_account(&vault_config).is_some());
}

#[test]
fn close_vault_rejects_wrong_pda_substitution() {
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
        close_vault_ix(user.pubkey(), other_vault_config)
    ));

    assert!(svm.get_account(&vault_config).is_some());
    assert!(svm.get_account(&other_vault_config).is_some());
}

#[test]
fn closed_vault_cannot_be_initialized_again() {
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
        close_vault_ix(user.pubkey(), vault_config)
    ));
    assert!(!send_instruction(
        &mut svm,
        &user,
        initialize_vault_ix(user.pubkey(), vault_config, DEFAULT_LIMIT_LAMPORTS)
    ));

    assert!(svm.get_account(&vault_config).is_none());
}
