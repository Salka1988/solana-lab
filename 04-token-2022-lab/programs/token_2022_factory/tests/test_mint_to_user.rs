use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::{
        spl_token_2022::{
            extension::{
                transfer_fee::TransferFeeAmount, BaseStateWithExtensions, ExtensionType,
                StateWithExtensions,
            },
            pod::PodAccount,
            state::{Account as TokenAccount, Mint},
        },
        ID as TOKEN_2022_PROGRAM_ID,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    token_2022_factory::Token2022MintConfig,
};

const INITIAL_AIRDROP_LAMPORTS: u64 = 1_000_000_000;
const TOKEN_DECIMALS: u8 = 6;
const TOKEN_NAME: &str = "Lab Stablecoin";
const TOKEN_SYMBOL: &str = "LABUSD";
const TOKEN_URI: &str = "https://example.com/lab-usd.json";
const TRANSFER_FEE_BASIS_POINTS: u16 = 25;
const MAXIMUM_FEE: u64 = 1_000_000;
const MINT_AMOUNT: u64 = 250_000_000;

fn setup() -> (LiteSVM, Keypair) {
    let program_id = token_2022_factory::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/token_2022_factory.so");

    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), INITIAL_AIRDROP_LAMPORTS)
        .unwrap();

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

fn read_token_account(svm: &LiteSVM, token_account: &Pubkey) -> TokenAccount {
    let account = svm
        .get_account(token_account)
        .expect("token account exists");
    let state = StateWithExtensions::<TokenAccount>::unpack(&account.data).unwrap();

    state.base
}

fn read_mint(svm: &LiteSVM, mint: &Pubkey) -> Mint {
    let account = svm.get_account(mint).expect("mint exists");
    let state = StateWithExtensions::<Mint>::unpack(&account.data).unwrap();

    state.base
}

#[test]
fn mint_to_user_creates_extended_token_account_and_mints_tokens() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let user = Keypair::new();
    let user_token_account = Keypair::new();
    let (mint_authority, _) = mint_authority_pda();

    let mint_config = initialize_mint(&mut svm, &admin, &mint);

    assert!(send_with_signers(
        &mut svm,
        &admin,
        &[&admin, &user_token_account],
        mint_to_user_ix(
            admin.pubkey(),
            mint_config,
            mint_authority,
            mint.pubkey(),
            user.pubkey(),
            user_token_account.pubkey(),
            MINT_AMOUNT,
        ),
    ));

    let token_account_info = svm
        .get_account(&user_token_account.pubkey())
        .expect("token account exists");
    let token_account_state =
        StateWithExtensions::<TokenAccount>::unpack(&token_account_info.data).unwrap();
    let transfer_fee_amount = token_account_state
        .get_extension::<TransferFeeAmount>()
        .unwrap();
    let expected_token_account_len =
        ExtensionType::try_calculate_account_len::<PodAccount>(&[ExtensionType::TransferFeeAmount])
            .unwrap();
    let token_account = read_token_account(&svm, &user_token_account.pubkey());
    let mint_state = read_mint(&svm, &mint.pubkey());

    assert_eq!(token_account_info.owner, TOKEN_2022_PROGRAM_ID);
    assert_eq!(token_account_info.data.len(), expected_token_account_len);
    assert_eq!(token_account.mint, mint.pubkey());
    assert_eq!(token_account.owner, user.pubkey());
    assert_eq!(u64::from(token_account.amount), MINT_AMOUNT);
    assert_eq!(u64::from(transfer_fee_amount.withheld_amount), 0);
    assert_eq!(u64::from(mint_state.supply), MINT_AMOUNT);

    let config_account = svm.get_account(&mint_config).expect("config exists");
    let config = Token2022MintConfig::try_deserialize(&mut config_account.data.as_slice()).unwrap();
    assert_eq!(config.mint, mint.pubkey());
}

#[test]
fn non_admin_cannot_mint_to_user() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let attacker = Keypair::new();
    let user = Keypair::new();
    let user_token_account = Keypair::new();
    let (mint_authority, _) = mint_authority_pda();

    svm.airdrop(&attacker.pubkey(), INITIAL_AIRDROP_LAMPORTS)
        .unwrap();

    let mint_config = initialize_mint(&mut svm, &admin, &mint);

    assert!(!send_with_signers(
        &mut svm,
        &attacker,
        &[&attacker, &user_token_account],
        mint_to_user_ix(
            attacker.pubkey(),
            mint_config,
            mint_authority,
            mint.pubkey(),
            user.pubkey(),
            user_token_account.pubkey(),
            MINT_AMOUNT,
        ),
    ));
}
