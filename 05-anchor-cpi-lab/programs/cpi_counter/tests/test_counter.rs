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

fn setup() -> (LiteSVM, Keypair) {
    let program_id = cpi_counter::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/cpi_counter.so");

    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), INITIAL_AIRDROP_LAMPORTS)
        .unwrap();

    (svm, payer)
}

fn fund_user(svm: &mut LiteSVM, user: &Keypair) {
    svm.airdrop(&user.pubkey(), INITIAL_AIRDROP_LAMPORTS)
        .unwrap();
}

fn counter_pda(authority: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[cpi_counter::COUNTER_SEED, authority.as_ref()],
        &cpi_counter::id(),
    )
}

fn initialize_counter_ix(authority: Pubkey, counter: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        cpi_counter::id(),
        &cpi_counter::instruction::InitializeCounter {}.data(),
        cpi_counter::accounts::InitializeCounterAccount {
            payer: authority,
            authority,
            counter,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn increment_ix(authority: Pubkey, counter: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        cpi_counter::id(),
        &cpi_counter::instruction::Increment {}.data(),
        cpi_counter::accounts::Increment { authority, counter }.to_account_metas(None),
    )
}

fn send_instruction(svm: &mut LiteSVM, signer: &Keypair, instruction: Instruction) -> bool {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();

    svm.send_transaction(tx).is_ok()
}

fn read_counter(svm: &LiteSVM, counter: &Pubkey) -> cpi_counter::Counter {
    let account = svm.get_account(counter).expect("counter account exists");

    cpi_counter::Counter::try_deserialize(&mut account.data.as_slice()).unwrap()
}

#[test]
fn initialize_counter_creates_expected_pda_state() {
    let (mut svm, authority) = setup();
    let (counter, expected_bump) = counter_pda(&authority.pubkey());

    assert!(send_instruction(
        &mut svm,
        &authority,
        initialize_counter_ix(authority.pubkey(), counter)
    ));

    let state = read_counter(&svm, &counter);

    assert_eq!(state.authority, authority.pubkey());
    assert_eq!(state.count, 0);
    assert_eq!(state.bump, expected_bump);
}

#[test]
fn increment_updates_counter() {
    let (mut svm, authority) = setup();
    let (counter, _) = counter_pda(&authority.pubkey());

    assert!(send_instruction(
        &mut svm,
        &authority,
        initialize_counter_ix(authority.pubkey(), counter)
    ));
    assert!(send_instruction(
        &mut svm,
        &authority,
        increment_ix(authority.pubkey(), counter)
    ));

    let state = read_counter(&svm, &counter);

    assert_eq!(state.count, 1);
}

#[test]
fn wrong_authority_cannot_increment_another_counter() {
    let (mut svm, authority) = setup();
    let attacker = Keypair::new();
    let (counter, _) = counter_pda(&authority.pubkey());

    fund_user(&mut svm, &attacker);

    assert!(send_instruction(
        &mut svm,
        &authority,
        initialize_counter_ix(authority.pubkey(), counter)
    ));

    assert!(!send_instruction(
        &mut svm,
        &attacker,
        increment_ix(attacker.pubkey(), counter)
    ));

    let state = read_counter(&svm, &counter);

    assert_eq!(state.count, 0);
}
