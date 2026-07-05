use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_m0_test_support::{
        assert_failure_contains, deserialize_account, fund_user, new_svm_with_programs,
        send_instruction, send_instruction_result,
    },
    solana_signer::Signer,
};

fn setup() -> (LiteSVM, Keypair) {
    new_svm_with_programs(&[
        (
            cpi_counter::id(),
            include_bytes!("../../../target/deploy/cpi_counter.so"),
        ),
        (
            cpi_proxy::id(),
            include_bytes!("../../../target/deploy/cpi_proxy.so"),
        ),
    ])
}

fn counter_pda(authority: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[cpi_counter::COUNTER_SEED, authority.as_ref()],
        &cpi_counter::id(),
    )
}

fn proxy_authority_pda(user: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[cpi_proxy::PROXY_AUTHORITY_SEED, user.as_ref()],
        &cpi_proxy::id(),
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

fn proxy_initialize_counter_ix(
    user: Pubkey,
    proxy_authority: Pubkey,
    counter: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        cpi_proxy::id(),
        &cpi_proxy::instruction::ProxyInitializeCounter {}.data(),
        cpi_proxy::accounts::ProxyInitializeCounter {
            user,
            proxy_authority,
            counter,
            cpi_counter_program: cpi_counter::id(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn proxy_increment_ix(authority: Pubkey, counter: Pubkey) -> Instruction {
    proxy_increment_ix_with_program(authority, counter, cpi_counter::id())
}

fn proxy_increment_with_signer_ix(
    user: Pubkey,
    proxy_authority: Pubkey,
    counter: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        cpi_proxy::id(),
        &cpi_proxy::instruction::ProxyIncrementWithSigner {}.data(),
        cpi_proxy::accounts::ProxyIncrementWithSigner {
            user,
            proxy_authority,
            counter,
            cpi_counter_program: cpi_counter::id(),
        }
        .to_account_metas(None),
    )
}

fn proxy_increment_ix_with_program(
    authority: Pubkey,
    counter: Pubkey,
    cpi_counter_program: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        cpi_proxy::id(),
        &cpi_proxy::instruction::ProxyIncrement {}.data(),
        cpi_proxy::accounts::ProxyIncrement {
            authority,
            counter,
            cpi_counter_program,
        }
        .to_account_metas(None),
    )
}

fn read_counter(svm: &LiteSVM, counter: &Pubkey) -> cpi_counter::Counter {
    deserialize_account(svm, counter)
}

#[test]
fn proxy_increment_calls_counter_program() {
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
        proxy_increment_ix(authority.pubkey(), counter)
    ));

    let state = read_counter(&svm, &counter);

    assert_eq!(state.count, 1);
}

#[test]
fn proxy_increment_still_rejects_wrong_authority() {
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
        proxy_increment_ix(attacker.pubkey(), counter)
    ));

    let state = read_counter(&svm, &counter);

    assert_eq!(state.count, 0);
}

#[test]
fn proxy_increment_rejects_wrong_counter_account() {
    let (mut svm, authority) = setup();
    let attacker = Keypair::new();
    let (authority_counter, _) = counter_pda(&authority.pubkey());
    let (attacker_counter, _) = counter_pda(&attacker.pubkey());

    fund_user(&mut svm, &attacker);

    assert!(send_instruction(
        &mut svm,
        &authority,
        initialize_counter_ix(authority.pubkey(), authority_counter)
    ));
    assert!(send_instruction(
        &mut svm,
        &attacker,
        initialize_counter_ix(attacker.pubkey(), attacker_counter)
    ));
    assert!(!send_instruction(
        &mut svm,
        &authority,
        proxy_increment_ix(authority.pubkey(), attacker_counter)
    ));

    let authority_state = read_counter(&svm, &authority_counter);
    let attacker_state = read_counter(&svm, &attacker_counter);

    assert_eq!(authority_state.count, 0);
    assert_eq!(attacker_state.count, 0);
}

#[test]
fn proxy_increment_rejects_wrong_callee_program() {
    let (mut svm, authority) = setup();
    let (counter, _) = counter_pda(&authority.pubkey());

    assert!(send_instruction(
        &mut svm,
        &authority,
        initialize_counter_ix(authority.pubkey(), counter)
    ));
    assert!(!send_instruction(
        &mut svm,
        &authority,
        proxy_increment_ix_with_program(authority.pubkey(), counter, cpi_proxy::id())
    ));

    let state = read_counter(&svm, &counter);

    assert_eq!(state.count, 0);
}

#[test]
fn proxy_signer_seeds_initialize_and_increment_counter() {
    let (mut svm, user) = setup();
    let (proxy_authority, _) = proxy_authority_pda(&user.pubkey());
    let (counter, _) = counter_pda(&proxy_authority);

    assert!(send_instruction(
        &mut svm,
        &user,
        proxy_initialize_counter_ix(user.pubkey(), proxy_authority, counter)
    ));
    assert!(send_instruction(
        &mut svm,
        &user,
        proxy_increment_with_signer_ix(user.pubkey(), proxy_authority, counter)
    ));

    let state = read_counter(&svm, &counter);

    assert_eq!(state.authority, proxy_authority);
    assert_eq!(state.count, 1);
}

#[test]
fn proxy_signer_seeds_reject_wrong_proxy_authority() {
    let (mut svm, user) = setup();
    let attacker = Keypair::new();
    let (proxy_authority, _) = proxy_authority_pda(&user.pubkey());
    let (wrong_proxy_authority, _) = proxy_authority_pda(&attacker.pubkey());
    let (counter, _) = counter_pda(&proxy_authority);

    assert!(!send_instruction(
        &mut svm,
        &user,
        proxy_initialize_counter_ix(user.pubkey(), wrong_proxy_authority, counter)
    ));
}

#[test]
fn proxy_error_shows_proxy_program_validation_failure() {
    let (mut svm, authority) = setup();
    let (counter, _) = counter_pda(&authority.pubkey());

    assert!(send_instruction(
        &mut svm,
        &authority,
        initialize_counter_ix(authority.pubkey(), counter)
    ));

    let failure = send_instruction_result(
        &mut svm,
        &authority,
        proxy_increment_ix_with_program(authority.pubkey(), counter, cpi_proxy::id()),
    )
    .unwrap_err();

    assert_failure_contains(&failure, "cpi_counter_program");
    assert_failure_contains(&failure, "InvalidProgramId");
}

#[test]
fn proxy_error_shows_callee_program_validation_failure() {
    let (mut svm, authority) = setup();
    let attacker = Keypair::new();
    let (authority_counter, _) = counter_pda(&authority.pubkey());
    let (attacker_counter, _) = counter_pda(&attacker.pubkey());

    fund_user(&mut svm, &attacker);

    assert!(send_instruction(
        &mut svm,
        &authority,
        initialize_counter_ix(authority.pubkey(), authority_counter)
    ));
    assert!(send_instruction(
        &mut svm,
        &attacker,
        initialize_counter_ix(attacker.pubkey(), attacker_counter)
    ));

    let failure = send_instruction_result(
        &mut svm,
        &authority,
        proxy_increment_ix(authority.pubkey(), attacker_counter),
    )
    .unwrap_err();

    assert_failure_contains(&failure, "Instruction: Increment");
    assert_failure_contains(&failure, "ConstraintSeeds");
}

#[test]
fn proxy_error_shows_signer_seed_validation_failure() {
    let (mut svm, user) = setup();
    let attacker = Keypair::new();
    let (proxy_authority, _) = proxy_authority_pda(&user.pubkey());
    let (wrong_proxy_authority, _) = proxy_authority_pda(&attacker.pubkey());
    let (counter, _) = counter_pda(&proxy_authority);

    let failure = send_instruction_result(
        &mut svm,
        &user,
        proxy_initialize_counter_ix(user.pubkey(), wrong_proxy_authority, counter),
    )
    .unwrap_err();

    assert_failure_contains(&failure, "proxy_authority");
    assert_failure_contains(&failure, "ConstraintSeeds");
}
