use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const INITIAL_AIRDROP_LAMPORTS: u64 = 1_000_000_000;
const DEFAULT_LIMIT_LAMPORTS: u64 = 500_000_000;

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

fn send_instruction(svm: &mut LiteSVM, payer: &Keypair, instruction: Instruction) -> bool {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();

    svm.send_transaction(tx).is_ok()
}

#[test]
fn initialize_vault_creates_expected_pda_state() {
    let (mut svm, user) = setup();
    let (vault_config, expected_bump) = vault_config_pda(&user.pubkey());
    let instruction = initialize_vault_ix(user.pubkey(), vault_config, DEFAULT_LIMIT_LAMPORTS);

    assert!(send_instruction(&mut svm, &user, instruction));

    let account = svm.get_account(&vault_config).expect("vault config exists");
    let vault_state =
        vault_authority::VaultConfig::try_deserialize(&mut account.data.as_slice()).unwrap();

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
