use {
    anchor_lang::{
        prelude::rent,
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{get_associated_token_address, ID as ASSOCIATED_TOKEN_PROGRAM_ID},
        token::{Mint, TokenAccount, ID as TOKEN_PROGRAM_ID},
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const INITIAL_AIRDROP_LAMPORTS: u64 = 1_000_000_000;
const REWARD_DECIMALS: u8 = 6;
const MINT_AMOUNT: u64 = 250_000_000;

fn setup() -> (LiteSVM, Keypair) {
    let program_id = reward_token::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/reward_token.so");

    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), INITIAL_AIRDROP_LAMPORTS)
        .unwrap();

    (svm, payer)
}

fn fund_user(svm: &mut LiteSVM, user: &Keypair) {
    svm.airdrop(&user.pubkey(), INITIAL_AIRDROP_LAMPORTS)
        .unwrap();
}

fn reward_mint_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[reward_token::REWARD_MINT_CONFIG_SEED],
        &reward_token::id(),
    )
}

fn mint_authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[reward_token::MINT_AUTHORITY_SEED], &reward_token::id())
}

fn initialize_reward_mint_ix(
    admin: Pubkey,
    reward_mint_config: Pubkey,
    mint_authority: Pubkey,
    reward_mint: Pubkey,
    decimals: u8,
) -> Instruction {
    Instruction::new_with_bytes(
        reward_token::id(),
        &reward_token::instruction::InitializeRewardMint { decimals }.data(),
        reward_token::accounts::InitializeRewardMint {
            admin,
            reward_mint_config,
            mint_authority,
            reward_mint,
            token_program: TOKEN_PROGRAM_ID,
            system_program: system_program::ID,
            rent: rent::ID,
        }
        .to_account_metas(None),
    )
}

fn ensure_user_ata_ix(
    user: Pubkey,
    reward_mint_config: Pubkey,
    reward_mint: Pubkey,
    user_ata: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        reward_token::id(),
        &reward_token::instruction::EnsureUserAta {}.data(),
        reward_token::accounts::EnsureUserAta {
            user,
            reward_mint_config,
            reward_mint,
            user_ata,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            token_program: TOKEN_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn mint_reward_ix(
    admin: Pubkey,
    reward_mint_config: Pubkey,
    reward_mint: Pubkey,
    mint_authority: Pubkey,
    recipient_ata: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        reward_token::id(),
        &reward_token::instruction::MintReward { amount }.data(),
        reward_token::accounts::MintReward {
            admin,
            reward_mint_config,
            reward_mint,
            mint_authority,
            recipient_ata,
            token_program: TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}

fn send_instruction(svm: &mut LiteSVM, signer: &Keypair, instruction: Instruction) -> bool {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();

    svm.send_transaction(tx).is_ok()
}

fn send_initialize_reward_mint(
    svm: &mut LiteSVM,
    admin: &Keypair,
    reward_mint: &Keypair,
    instruction: Instruction,
) -> bool {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&admin.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[admin, reward_mint])
        .unwrap();

    svm.send_transaction(tx).is_ok()
}

fn initialize_reward_mint(svm: &mut LiteSVM, admin: &Keypair, reward_mint: &Keypair) -> Pubkey {
    let (reward_mint_config, _) = reward_mint_config_pda();
    let (mint_authority, _) = mint_authority_pda();

    assert!(send_initialize_reward_mint(
        svm,
        admin,
        reward_mint,
        initialize_reward_mint_ix(
            admin.pubkey(),
            reward_mint_config,
            mint_authority,
            reward_mint.pubkey(),
            REWARD_DECIMALS
        )
    ));

    reward_mint_config
}

fn ensure_user_ata(
    svm: &mut LiteSVM,
    user: &Keypair,
    reward_mint_config: Pubkey,
    reward_mint: Pubkey,
) -> Pubkey {
    let user_ata = get_associated_token_address(&user.pubkey(), &reward_mint);

    assert!(send_instruction(
        svm,
        user,
        ensure_user_ata_ix(user.pubkey(), reward_mint_config, reward_mint, user_ata)
    ));

    user_ata
}

fn read_token_account(svm: &LiteSVM, token_account: &Pubkey) -> TokenAccount {
    let account = svm
        .get_account(token_account)
        .expect("token account exists");

    TokenAccount::try_deserialize_unchecked(&mut account.data.as_slice()).unwrap()
}

fn read_mint(svm: &LiteSVM, mint: &Pubkey) -> Mint {
    let account = svm.get_account(mint).expect("mint exists");

    Mint::try_deserialize_unchecked(&mut account.data.as_slice()).unwrap()
}

#[test]
fn mint_reward_mints_tokens_to_user_ata() {
    let (mut svm, admin) = setup();
    let user = Keypair::new();
    let reward_mint = Keypair::new();
    let (mint_authority, _) = mint_authority_pda();

    fund_user(&mut svm, &user);

    let reward_mint_config = initialize_reward_mint(&mut svm, &admin, &reward_mint);
    let user_ata = ensure_user_ata(&mut svm, &user, reward_mint_config, reward_mint.pubkey());

    assert!(send_instruction(
        &mut svm,
        &admin,
        mint_reward_ix(
            admin.pubkey(),
            reward_mint_config,
            reward_mint.pubkey(),
            mint_authority,
            user_ata,
            MINT_AMOUNT
        )
    ));

    let token_state = read_token_account(&svm, &user_ata);
    let mint_state = read_mint(&svm, &reward_mint.pubkey());

    assert_eq!(token_state.amount, MINT_AMOUNT);
    assert_eq!(mint_state.supply, MINT_AMOUNT);
}

#[test]
fn mint_reward_rejects_unauthorized_admin() {
    let (mut svm, admin) = setup();
    let attacker = Keypair::new();
    let user = Keypair::new();
    let reward_mint = Keypair::new();
    let (mint_authority, _) = mint_authority_pda();

    fund_user(&mut svm, &attacker);
    fund_user(&mut svm, &user);

    let reward_mint_config = initialize_reward_mint(&mut svm, &admin, &reward_mint);
    let user_ata = ensure_user_ata(&mut svm, &user, reward_mint_config, reward_mint.pubkey());

    assert!(!send_instruction(
        &mut svm,
        &attacker,
        mint_reward_ix(
            attacker.pubkey(),
            reward_mint_config,
            reward_mint.pubkey(),
            mint_authority,
            user_ata,
            MINT_AMOUNT
        )
    ));

    let token_state = read_token_account(&svm, &user_ata);
    let mint_state = read_mint(&svm, &reward_mint.pubkey());

    assert_eq!(token_state.amount, 0);
    assert_eq!(mint_state.supply, 0);
}

#[test]
fn mint_reward_rejects_wrong_mint_authority_pda() {
    let (mut svm, admin) = setup();
    let user = Keypair::new();
    let reward_mint = Keypair::new();
    let wrong_mint_authority = Pubkey::new_unique();

    fund_user(&mut svm, &user);

    let reward_mint_config = initialize_reward_mint(&mut svm, &admin, &reward_mint);
    let user_ata = ensure_user_ata(&mut svm, &user, reward_mint_config, reward_mint.pubkey());

    assert!(!send_instruction(
        &mut svm,
        &admin,
        mint_reward_ix(
            admin.pubkey(),
            reward_mint_config,
            reward_mint.pubkey(),
            wrong_mint_authority,
            user_ata,
            MINT_AMOUNT
        )
    ));
}
