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
    test_support::new_svm_with_program(
        token_2022_factory::id(),
        include_bytes!("../../../target/deploy/token_2022_factory.so"),
    )
}

#[test]
fn initialize_scaffold_succeeds() {
    let (mut svm, payer) = setup();
    let instruction = Instruction::new_with_bytes(
        token_2022_factory::id(),
        &token_2022_factory::instruction::Initialize {}.data(),
        token_2022_factory::accounts::Initialize {}.to_account_metas(None),
    );
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer]).unwrap();

    assert!(svm.send_transaction(tx).is_ok());
}
