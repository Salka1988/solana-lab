use {
    anchor_lang::{prelude::Pubkey, solana_program::instruction::Instruction, AccountDeserialize},
    anchor_spl::token_2022::spl_token_2022::{
        extension::StateWithExtensions,
        state::{Account as TokenAccount, Mint},
    },
    litesvm::{types::FailedTransactionMetadata, LiteSVM},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

pub const INITIAL_AIRDROP_LAMPORTS: u64 = 1_000_000_000;

pub fn new_svm_with_payer() -> (LiteSVM, Keypair) {
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();

    fund_user(&mut svm, &payer);

    (svm, payer)
}

pub fn add_program(svm: &mut LiteSVM, program_id: Pubkey, bytes: &[u8]) {
    svm.add_program(program_id, bytes).unwrap();
}

pub fn fund_user(svm: &mut LiteSVM, user: &Keypair) {
    svm.airdrop(&user.pubkey(), INITIAL_AIRDROP_LAMPORTS)
        .unwrap();
}

pub fn send_instruction(svm: &mut LiteSVM, signer: &Keypair, instruction: Instruction) -> bool {
    send_instruction_result(svm, signer, instruction).is_ok()
}

pub fn send_instruction_with_signers(
    svm: &mut LiteSVM,
    fee_payer: Pubkey,
    instruction: Instruction,
    signers: &[&Keypair],
) -> bool {
    send_instruction_with_signers_result(svm, fee_payer, instruction, signers).is_ok()
}

pub fn send_instruction_result(
    svm: &mut LiteSVM,
    signer: &Keypair,
    instruction: Instruction,
) -> Result<(), FailedTransactionMetadata> {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();

    svm.send_transaction(tx).map(|_| ())
}

pub fn send_instruction_with_signers_result(
    svm: &mut LiteSVM,
    fee_payer: Pubkey,
    instruction: Instruction,
    signers: &[&Keypair],
) -> Result<(), FailedTransactionMetadata> {
    send_transaction(svm, fee_payer, vec![instruction], signers)
}

pub fn send_transaction(
    svm: &mut LiteSVM,
    fee_payer: Pubkey,
    instructions: Vec<Instruction>,
    signers: &[&Keypair],
) -> Result<(), FailedTransactionMetadata> {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&instructions, Some(&fee_payer), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();

    svm.send_transaction(tx).map(|_| ())
}

pub fn assert_failure_contains(failure: &FailedTransactionMetadata, expected: &str) {
    let failure_text = format!("{:?}\n{}", failure.err, failure.meta.pretty_logs());

    assert!(
        failure_text.contains(expected),
        "expected failure to contain `{expected}`\nactual failure:\n{failure_text}"
    );
}

pub fn deserialize_account<T: AccountDeserialize>(svm: &LiteSVM, address: &Pubkey) -> T {
    let account = svm.get_account(address).expect("account exists");

    T::try_deserialize(&mut account.data.as_slice()).unwrap()
}

pub fn deserialize_account_unchecked<T: AccountDeserialize>(svm: &LiteSVM, address: &Pubkey) -> T {
    let account = svm.get_account(address).expect("account exists");

    T::try_deserialize_unchecked(&mut account.data.as_slice()).unwrap()
}

pub fn token_2022_account_amount(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let account = svm.get_account(token_account).expect("token account exists");
    let state = StateWithExtensions::<TokenAccount>::unpack(&account.data).unwrap();

    state.base.amount
}

pub fn token_2022_mint_supply(svm: &LiteSVM, mint: &Pubkey) -> u64 {
    let account = svm.get_account(mint).expect("mint exists");
    let state = StateWithExtensions::<Mint>::unpack(&account.data).unwrap();

    state.base.supply
}
