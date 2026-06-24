use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const INITIAL_AIRDROP_LAMPORTS: u64 = 1_000_000_000;

fn setup() -> (LiteSVM, Keypair) {
    let program_id = reward_token::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/reward_token.so");

    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), INITIAL_AIRDROP_LAMPORTS)
        .unwrap();

    (svm, payer)
}

#[test]
fn initialize_scaffold_succeeds() {
    let (mut svm, payer) = setup();
    let instruction = Instruction::new_with_bytes(
        reward_token::id(),
        &reward_token::instruction::Initialize {}.data(),
        reward_token::accounts::Initialize {}.to_account_metas(None),
    );
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer]).unwrap();

    assert!(svm.send_transaction(tx).is_ok());
}
