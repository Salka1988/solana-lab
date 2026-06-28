use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{clock::Clock, instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::{
        spl_token_2022::{
            extension::{
                transfer_fee::TransferFeeConfig, BaseStateWithExtensions, StateWithExtensions,
            },
            state::Mint,
        },
        ID as TOKEN_2022_PROGRAM_ID,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const TOKEN_DECIMALS: u8 = 6;
const TOKEN_NAME: &str = "Lab Stablecoin";
const TOKEN_SYMBOL: &str = "LABUSD";
const TOKEN_URI: &str = "https://example.com/lab-usd.json";
const INITIAL_TRANSFER_FEE_BASIS_POINTS: u16 = 25;
const INITIAL_MAXIMUM_FEE: u64 = 1_000_000;
const UPDATED_TRANSFER_FEE_BASIS_POINTS: u16 = 100;
const UPDATED_MAXIMUM_FEE: u64 = 2_000_000;
const INVALID_TRANSFER_FEE_BASIS_POINTS: u16 = 10_001;

fn setup() -> (LiteSVM, Keypair) {
    let (mut svm, payer) = test_support::new_svm_with_payer();
    let bytes = include_bytes!("../../../target/deploy/token_2022_factory.so");

    test_support::add_program(&mut svm, token_2022_factory::id(), bytes);

    (svm, payer)
}

fn mint_config_pda(mint: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            token_2022_factory::TOKEN_2022_MINT_CONFIG_SEED,
            mint.as_ref(),
        ],
        &token_2022_factory::id(),
    )
}

fn mint_authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[token_2022_factory::MINT_AUTHORITY_SEED],
        &token_2022_factory::id(),
    )
}

fn create_token_2022_mint_ix(
    admin: Pubkey,
    mint_config: Pubkey,
    mint_authority: Pubkey,
    mint: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        token_2022_factory::id(),
        &token_2022_factory::instruction::CreateToken2022Mint {
            decimals: TOKEN_DECIMALS,
            name: TOKEN_NAME.to_string(),
            symbol: TOKEN_SYMBOL.to_string(),
            uri: TOKEN_URI.to_string(),
            transfer_fee_basis_points: INITIAL_TRANSFER_FEE_BASIS_POINTS,
            maximum_fee: INITIAL_MAXIMUM_FEE,
        }
        .data(),
        token_2022_factory::accounts::CreateToken2022Mint {
            admin,
            mint_config,
            mint_authority,
            mint,
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn set_transfer_fee_config_ix(
    admin: Pubkey,
    mint_config: Pubkey,
    mint_authority: Pubkey,
    mint: Pubkey,
    transfer_fee_basis_points: u16,
    maximum_fee: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        token_2022_factory::id(),
        &token_2022_factory::instruction::SetTransferFeeConfig {
            transfer_fee_basis_points,
            maximum_fee,
        }
        .data(),
        token_2022_factory::accounts::SetTransferFeeConfig {
            admin,
            mint_config,
            mint_authority,
            mint,
            token_program: TOKEN_2022_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}

fn send_with_signers(
    svm: &mut LiteSVM,
    payer: &Keypair,
    signers: &[&Keypair],
    instruction: Instruction,
) -> bool {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();

    match svm.send_transaction(tx) {
        Ok(meta) => {
            println!("{}", meta.pretty_logs());
            true
        }
        Err(err) => {
            println!("{err:#?}");
            false
        }
    }
}

fn initialize_mint(svm: &mut LiteSVM, admin: &Keypair, mint: &Keypair) -> Pubkey {
    let (mint_config, _) = mint_config_pda(mint.pubkey());
    let (mint_authority, _) = mint_authority_pda();

    assert!(send_with_signers(
        svm,
        admin,
        &[admin, mint],
        create_token_2022_mint_ix(admin.pubkey(), mint_config, mint_authority, mint.pubkey()),
    ));

    mint_config
}

fn read_transfer_fee_config(svm: &LiteSVM, mint: &Pubkey) -> TransferFeeConfig {
    let account = svm.get_account(mint).expect("mint exists");
    let state = StateWithExtensions::<Mint>::unpack(&account.data).unwrap();

    *state.get_extension::<TransferFeeConfig>().unwrap()
}

#[test]
fn admin_can_schedule_transfer_fee_update() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let mint_config = initialize_mint(&mut svm, &admin, &mint);
    let (mint_authority, _) = mint_authority_pda();
    let current_epoch = svm.get_sysvar::<Clock>().epoch;

    assert!(send_with_signers(
        &mut svm,
        &admin,
        &[&admin],
        set_transfer_fee_config_ix(
            admin.pubkey(),
            mint_config,
            mint_authority,
            mint.pubkey(),
            UPDATED_TRANSFER_FEE_BASIS_POINTS,
            UPDATED_MAXIMUM_FEE,
        ),
    ));

    let fee_config = read_transfer_fee_config(&svm, &mint.pubkey());

    assert_eq!(
        u16::from(fee_config.older_transfer_fee.transfer_fee_basis_points),
        INITIAL_TRANSFER_FEE_BASIS_POINTS
    );
    assert_eq!(
        u16::from(fee_config.newer_transfer_fee.transfer_fee_basis_points),
        UPDATED_TRANSFER_FEE_BASIS_POINTS
    );
    assert_eq!(
        u64::from(fee_config.newer_transfer_fee.maximum_fee),
        UPDATED_MAXIMUM_FEE
    );
    assert_eq!(
        u64::from(fee_config.newer_transfer_fee.epoch),
        current_epoch + 2
    );
}

#[test]
fn non_admin_cannot_schedule_transfer_fee_update() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let attacker = Keypair::new();
    let mint_config = initialize_mint(&mut svm, &admin, &mint);
    let (mint_authority, _) = mint_authority_pda();

    assert!(!send_with_signers(
        &mut svm,
        &admin,
        &[&admin, &attacker],
        set_transfer_fee_config_ix(
            attacker.pubkey(),
            mint_config,
            mint_authority,
            mint.pubkey(),
            UPDATED_TRANSFER_FEE_BASIS_POINTS,
            UPDATED_MAXIMUM_FEE,
        ),
    ));
}

#[test]
fn invalid_transfer_fee_basis_points_are_rejected() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let mint_config = initialize_mint(&mut svm, &admin, &mint);
    let (mint_authority, _) = mint_authority_pda();

    assert!(!send_with_signers(
        &mut svm,
        &admin,
        &[&admin],
        set_transfer_fee_config_ix(
            admin.pubkey(),
            mint_config,
            mint_authority,
            mint.pubkey(),
            INVALID_TRANSFER_FEE_BASIS_POINTS,
            UPDATED_MAXIMUM_FEE,
        ),
    ));
}
