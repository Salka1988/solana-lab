use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::{AccountMeta, Pubkey},
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::{
        spl_token_2022::{
            extension::{
                transfer_fee::{TransferFeeAmount, TransferFeeConfig},
                BaseStateWithExtensions, StateWithExtensions,
            },
            state::{Account as TokenAccount, Mint},
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
const TRANSFER_FEE_BASIS_POINTS: u16 = 25;
const MAXIMUM_FEE: u64 = 1_000_000;
const MINT_AMOUNT: u64 = 250_000_000;
const TRANSFER_AMOUNT: u64 = 100_000_000;
const EXPECTED_FEE: u64 = 250_000;

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

fn transfer_with_fee_ix(
    owner: Pubkey,
    mint_config: Pubkey,
    mint: Pubkey,
    source: Pubkey,
    destination: Pubkey,
    amount: u64,
    fee: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        token_2022_factory::id(),
        &token_2022_factory::instruction::TransferWithFee { amount, fee }.data(),
        token_2022_factory::accounts::TransferWithFee {
            owner,
            mint_config,
            mint,
            source,
            destination,
            token_program: TOKEN_2022_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}

fn harvest_withheld_fees_ix(mint_config: Pubkey, mint: Pubkey, sources: &[Pubkey]) -> Instruction {
    let mut accounts = token_2022_factory::accounts::HarvestWithheldFees {
        mint_config,
        mint,
        token_program: TOKEN_2022_PROGRAM_ID,
    }
    .to_account_metas(None);

    accounts.extend(
        sources
            .iter()
            .map(|source| AccountMeta::new(*source, false)),
    );

    Instruction::new_with_bytes(
        token_2022_factory::id(),
        &token_2022_factory::instruction::HarvestWithheldFees {}.data(),
        accounts,
    )
}

fn withdraw_withheld_fees_ix(
    admin: Pubkey,
    mint_config: Pubkey,
    mint_authority: Pubkey,
    mint: Pubkey,
    destination: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        token_2022_factory::id(),
        &token_2022_factory::instruction::WithdrawWithheldFees {}.data(),
        token_2022_factory::accounts::WithdrawWithheldFees {
            admin,
            mint_config,
            mint_authority,
            mint,
            destination,
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

fn create_fee_state() -> (LiteSVM, Keypair, Keypair, Pubkey, Keypair, Keypair, Keypair) {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let alice = Keypair::new();
    let bob = Keypair::new();
    let admin_fee_account = Keypair::new();
    let alice_token_account = Keypair::new();
    let bob_token_account = Keypair::new();

    let mint_config = initialize_mint(&mut svm, &admin, &mint);
    create_user_token_account(
        &mut svm,
        &admin,
        mint_config,
        mint.pubkey(),
        alice.pubkey(),
        &alice_token_account,
        MINT_AMOUNT,
    );
    create_user_token_account(
        &mut svm,
        &admin,
        mint_config,
        mint.pubkey(),
        bob.pubkey(),
        &bob_token_account,
        0,
    );
    create_user_token_account(
        &mut svm,
        &admin,
        mint_config,
        mint.pubkey(),
        admin.pubkey(),
        &admin_fee_account,
        0,
    );

    assert!(send_with_signers(
        &mut svm,
        &admin,
        &[&admin, &alice],
        transfer_with_fee_ix(
            alice.pubkey(),
            mint_config,
            mint.pubkey(),
            alice_token_account.pubkey(),
            bob_token_account.pubkey(),
            TRANSFER_AMOUNT,
            EXPECTED_FEE,
        ),
    ));

    (
        svm,
        admin,
        mint,
        mint_config,
        bob_token_account,
        admin_fee_account,
        alice,
    )
}

fn read_token_account_with_fee(
    svm: &LiteSVM,
    token_account: &Pubkey,
) -> (TokenAccount, TransferFeeAmount) {
    let account = svm
        .get_account(token_account)
        .expect("token account exists");
    let state = StateWithExtensions::<TokenAccount>::unpack(&account.data).unwrap();
    let fee_amount = *state.get_extension::<TransferFeeAmount>().unwrap();

    (state.base, fee_amount)
}

fn read_mint_fee_config(svm: &LiteSVM, mint: &Pubkey) -> TransferFeeConfig {
    let account = svm.get_account(mint).expect("mint exists");
    let state = StateWithExtensions::<Mint>::unpack(&account.data).unwrap();

    *state.get_extension::<TransferFeeConfig>().unwrap()
}

#[test]
fn harvest_moves_withheld_fees_from_account_to_mint() {
    let (mut svm, admin, mint, mint_config, bob_token_account, _, _) = create_fee_state();

    let (_, bob_fee_before) = read_token_account_with_fee(&svm, &bob_token_account.pubkey());
    let mint_fee_before = read_mint_fee_config(&svm, &mint.pubkey());
    assert_eq!(u64::from(bob_fee_before.withheld_amount), EXPECTED_FEE);
    assert_eq!(u64::from(mint_fee_before.withheld_amount), 0);

    assert!(send_with_signers(
        &mut svm,
        &admin,
        &[&admin],
        harvest_withheld_fees_ix(mint_config, mint.pubkey(), &[bob_token_account.pubkey()]),
    ));

    let (_, bob_fee_after) = read_token_account_with_fee(&svm, &bob_token_account.pubkey());
    let mint_fee_after = read_mint_fee_config(&svm, &mint.pubkey());
    assert_eq!(u64::from(bob_fee_after.withheld_amount), 0);
    assert_eq!(u64::from(mint_fee_after.withheld_amount), EXPECTED_FEE);
}

#[test]
fn withdraw_moves_mint_withheld_fees_to_destination() {
    let (mut svm, admin, mint, mint_config, bob_token_account, admin_fee_account, _) =
        create_fee_state();
    let (mint_authority, _) = mint_authority_pda();

    assert!(send_with_signers(
        &mut svm,
        &admin,
        &[&admin],
        harvest_withheld_fees_ix(mint_config, mint.pubkey(), &[bob_token_account.pubkey()]),
    ));
    assert!(send_with_signers(
        &mut svm,
        &admin,
        &[&admin],
        withdraw_withheld_fees_ix(
            admin.pubkey(),
            mint_config,
            mint_authority,
            mint.pubkey(),
            admin_fee_account.pubkey(),
        ),
    ));

    let (admin_fee_token_account, admin_fee_amount) =
        read_token_account_with_fee(&svm, &admin_fee_account.pubkey());
    let mint_fee_after = read_mint_fee_config(&svm, &mint.pubkey());

    assert_eq!(u64::from(admin_fee_token_account.amount), EXPECTED_FEE);
    assert_eq!(u64::from(admin_fee_amount.withheld_amount), 0);
    assert_eq!(u64::from(mint_fee_after.withheld_amount), 0);
}

#[test]
fn non_admin_cannot_withdraw_mint_withheld_fees() {
    let (mut svm, admin, mint, mint_config, bob_token_account, admin_fee_account, attacker) =
        create_fee_state();
    let (mint_authority, _) = mint_authority_pda();

    assert!(send_with_signers(
        &mut svm,
        &admin,
        &[&admin],
        harvest_withheld_fees_ix(mint_config, mint.pubkey(), &[bob_token_account.pubkey()]),
    ));

    assert!(!send_with_signers(
        &mut svm,
        &admin,
        &[&admin, &attacker],
        withdraw_withheld_fees_ix(
            attacker.pubkey(),
            mint_config,
            mint_authority,
            mint.pubkey(),
            admin_fee_account.pubkey(),
        ),
    ));
}
