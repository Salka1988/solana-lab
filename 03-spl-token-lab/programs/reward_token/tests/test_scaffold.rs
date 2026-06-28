use solana_m0_test_support as test_support;
use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn setup() -> (LiteSVM, Keypair) {
    let (mut svm, payer) = test_support::new_svm_with_payer();
    let bytes = include_bytes!("../../../target/deploy/reward_token.so");

    test_support::add_program(&mut svm, reward_token::id(), bytes);

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
