use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::{
        spl_token_2022::state::{Account as TokenAccount, Mint},
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
const TRANSFER_FEE_BASIS_POINTS: u16 = 25;
const MAXIMUM_FEE: u64 = 1_000_000;
const MINT_AMOUNT: u64 = 250_000_000;
const BURN_AMOUNT: u64 = 40_000_000;

fn setup() -> (LiteSVM, Keypair) {
    test_support::new_svm_with_program(
        token_2022_factory::id(),
        include_bytes!("../../../target/deploy/token_2022_factory.so"),
    )
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
            transfer_fee_basis_points: TRANSFER_FEE_BASIS_POINTS,
            maximum_fee: MAXIMUM_FEE,
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

fn mint_to_user_ix(
    admin: Pubkey,
    mint_config: Pubkey,
    mint_authority: Pubkey,
    mint: Pubkey,
    user: Pubkey,
    user_token_account: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        token_2022_factory::id(),
        &token_2022_factory::instruction::MintToUser { amount }.data(),
        token_2022_factory::accounts::MintToUser {
            admin,
            mint_config,
            mint_authority,
            mint,
            user,
            user_token_account,
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn burn_from_user_ix(
    owner: Pubkey,
    mint_config: Pubkey,
    mint: Pubkey,
    user_token_account: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        token_2022_factory::id(),
        &token_2022_factory::instruction::BurnFromUser { amount }.data(),
        token_2022_factory::accounts::BurnFromUser {
            owner,
            mint_config,
            mint,
            user_token_account,
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

fn create_user_token_account(
    svm: &mut LiteSVM,
    admin: &Keypair,
    mint_config: Pubkey,
    mint: Pubkey,
    user: Pubkey,
    token_account: &Keypair,
    amount: u64,
) {
    let (mint_authority, _) = mint_authority_pda();

    assert!(send_with_signers(
        svm,
        admin,
        &[admin, token_account],
        mint_to_user_ix(
            admin.pubkey(),
            mint_config,
            mint_authority,
            mint,
            user,
            token_account.pubkey(),
            amount,
        ),
    ));
}

fn read_token_account(svm: &LiteSVM, token_account: &Pubkey) -> TokenAccount {
    test_support::token_2022_account(svm, token_account)
}

fn read_mint(svm: &LiteSVM, mint: &Pubkey) -> Mint {
    test_support::token_2022_mint(svm, mint)
}

#[test]
fn burn_from_user_reduces_balance_and_supply() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let user = Keypair::new();
    let user_token_account = Keypair::new();

    let mint_config = initialize_mint(&mut svm, &admin, &mint);
    create_user_token_account(
        &mut svm,
        &admin,
        mint_config,
        mint.pubkey(),
        user.pubkey(),
        &user_token_account,
        MINT_AMOUNT,
    );

    assert!(send_with_signers(
        &mut svm,
        &admin,
        &[&admin, &user],
        burn_from_user_ix(
            user.pubkey(),
            mint_config,
            mint.pubkey(),
            user_token_account.pubkey(),
            BURN_AMOUNT,
        ),
    ));

    let token_account = read_token_account(&svm, &user_token_account.pubkey());
    let mint_state = read_mint(&svm, &mint.pubkey());

    assert_eq!(u64::from(token_account.amount), MINT_AMOUNT - BURN_AMOUNT);
    assert_eq!(u64::from(mint_state.supply), MINT_AMOUNT - BURN_AMOUNT);
}

#[test]
fn over_burn_is_rejected() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let user = Keypair::new();
    let user_token_account = Keypair::new();

    let mint_config = initialize_mint(&mut svm, &admin, &mint);
    create_user_token_account(
        &mut svm,
        &admin,
        mint_config,
        mint.pubkey(),
        user.pubkey(),
        &user_token_account,
        MINT_AMOUNT,
    );

    assert!(!send_with_signers(
        &mut svm,
        &admin,
        &[&admin, &user],
        burn_from_user_ix(
            user.pubkey(),
            mint_config,
            mint.pubkey(),
            user_token_account.pubkey(),
            MINT_AMOUNT + 1,
        ),
    ));
}

#[test]
fn wrong_owner_cannot_burn_user_tokens() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let user = Keypair::new();
    let attacker = Keypair::new();
    let user_token_account = Keypair::new();

    let mint_config = initialize_mint(&mut svm, &admin, &mint);
    create_user_token_account(
        &mut svm,
        &admin,
        mint_config,
        mint.pubkey(),
        user.pubkey(),
        &user_token_account,
        MINT_AMOUNT,
    );

    assert!(!send_with_signers(
        &mut svm,
        &admin,
        &[&admin, &attacker],
        burn_from_user_ix(
            attacker.pubkey(),
            mint_config,
            mint.pubkey(),
            user_token_account.pubkey(),
            BURN_AMOUNT,
        ),
    ));
}
