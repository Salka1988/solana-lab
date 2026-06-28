use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_m0_test_support::{
        add_program, deserialize_account, fund_user, new_svm_with_payer, send_instruction,
    },
    solana_signer::Signer,
};

fn setup() -> (LiteSVM, Keypair) {
    let (mut svm, payer) = new_svm_with_payer();
    let bytes = include_bytes!("../../../target/deploy/cpi_counter.so");

    add_program(&mut svm, cpi_counter::id(), bytes);

    (svm, payer)
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

fn read_counter(svm: &LiteSVM, counter: &Pubkey) -> cpi_counter::Counter {
    deserialize_account(svm, counter)
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
