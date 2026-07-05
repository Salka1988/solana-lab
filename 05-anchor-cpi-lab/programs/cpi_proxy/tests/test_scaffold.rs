use {
    anchor_lang::{
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_m0_test_support::{new_svm_with_program, send_instruction},
};

fn setup() -> (LiteSVM, Keypair) {
    new_svm_with_program(
        cpi_proxy::id(),
        include_bytes!("../../../target/deploy/cpi_proxy.so"),
    )
}

#[test]
fn initialize_scaffold_succeeds() {
    let (mut svm, payer) = setup();
    let instruction = Instruction::new_with_bytes(
        cpi_proxy::id(),
        &cpi_proxy::instruction::Initialize {}.data(),
        cpi_proxy::accounts::InitializeProxy {
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    assert!(send_instruction(&mut svm, &payer, instruction));
}
