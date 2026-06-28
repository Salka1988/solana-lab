use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::rent,
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{get_associated_token_address, ID as ASSOCIATED_TOKEN_PROGRAM_ID},
        token::{TokenAccount, ID as TOKEN_PROGRAM_ID},
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};
const REWARD_DECIMALS: u8 = 6;
const MINT_AMOUNT: u64 = 250_000_000;
const TRANSFER_AMOUNT: u64 = 75_000_000;

fn setup() -> (LiteSVM, Keypair) {
    let (mut svm, payer) = test_support::new_svm_with_payer();
    let bytes = include_bytes!("../../../target/deploy/reward_token.so");

    test_support::add_program(&mut svm, reward_token::id(), bytes);

    (svm, payer)
}

fn fund_user(svm: &mut LiteSVM, user: &Keypair) {
    test_support::fund_user(svm, user);
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

fn transfer_reward_ix(
    sender: Pubkey,
    reward_mint_config: Pubkey,
    reward_mint: Pubkey,
    sender_ata: Pubkey,
    recipient_ata: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        reward_token::id(),
        &reward_token::instruction::TransferReward { amount }.data(),
        reward_token::accounts::TransferReward {
            sender,
            reward_mint_config,
            reward_mint,
            sender_ata,
            recipient_ata,
            token_program: TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}

fn send_instruction(svm: &mut LiteSVM, signer: &Keypair, instruction: Instruction) -> bool {
    test_support::send_instruction(svm, signer, instruction)
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

fn mint_to_user(
    svm: &mut LiteSVM,
    admin: &Keypair,
    reward_mint_config: Pubkey,
    reward_mint: Pubkey,
    user_ata: Pubkey,
) {
    let (mint_authority, _) = mint_authority_pda();

    assert!(send_instruction(
        svm,
        admin,
        mint_reward_ix(
            admin.pubkey(),
            reward_mint_config,
            reward_mint,
            mint_authority,
            user_ata,
            MINT_AMOUNT
        )
    ));
}

fn read_token_account(svm: &LiteSVM, token_account: &Pubkey) -> TokenAccount {
    let account = svm
        .get_account(token_account)
        .expect("token account exists");

    TokenAccount::try_deserialize_unchecked(&mut account.data.as_slice()).unwrap()
}

#[test]
fn transfer_reward_moves_tokens_between_user_atas() {
    let (mut svm, admin) = setup();
    let sender = Keypair::new();
    let recipient = Keypair::new();
    let reward_mint = Keypair::new();

    fund_user(&mut svm, &sender);
    fund_user(&mut svm, &recipient);

    let reward_mint_config = initialize_reward_mint(&mut svm, &admin, &reward_mint);
    let sender_ata = ensure_user_ata(&mut svm, &sender, reward_mint_config, reward_mint.pubkey());
    let recipient_ata = ensure_user_ata(
        &mut svm,
        &recipient,
        reward_mint_config,
        reward_mint.pubkey(),
    );
    mint_to_user(
        &mut svm,
        &admin,
        reward_mint_config,
        reward_mint.pubkey(),
        sender_ata,
    );

    assert!(send_instruction(
        &mut svm,
        &sender,
        transfer_reward_ix(
            sender.pubkey(),
            reward_mint_config,
            reward_mint.pubkey(),
            sender_ata,
            recipient_ata,
            TRANSFER_AMOUNT
        )
    ));

    let sender_state = read_token_account(&svm, &sender_ata);
    let recipient_state = read_token_account(&svm, &recipient_ata);

    assert_eq!(sender_state.amount, MINT_AMOUNT - TRANSFER_AMOUNT);
    assert_eq!(recipient_state.amount, TRANSFER_AMOUNT);
}

#[test]
fn transfer_reward_rejects_amount_over_balance() {
    let (mut svm, admin) = setup();
    let sender = Keypair::new();
    let recipient = Keypair::new();
    let reward_mint = Keypair::new();

    fund_user(&mut svm, &sender);
    fund_user(&mut svm, &recipient);

    let reward_mint_config = initialize_reward_mint(&mut svm, &admin, &reward_mint);
    let sender_ata = ensure_user_ata(&mut svm, &sender, reward_mint_config, reward_mint.pubkey());
    let recipient_ata = ensure_user_ata(
        &mut svm,
        &recipient,
        reward_mint_config,
        reward_mint.pubkey(),
    );
    mint_to_user(
        &mut svm,
        &admin,
        reward_mint_config,
        reward_mint.pubkey(),
        sender_ata,
    );

    assert!(!send_instruction(
        &mut svm,
        &sender,
        transfer_reward_ix(
            sender.pubkey(),
            reward_mint_config,
            reward_mint.pubkey(),
            sender_ata,
            recipient_ata,
            MINT_AMOUNT + 1
        )
    ));

    let sender_state = read_token_account(&svm, &sender_ata);
    let recipient_state = read_token_account(&svm, &recipient_ata);

    assert_eq!(sender_state.amount, MINT_AMOUNT);
    assert_eq!(recipient_state.amount, 0);
}

#[test]
fn transfer_reward_rejects_unauthorized_sender() {
    let (mut svm, admin) = setup();
    let sender = Keypair::new();
    let recipient = Keypair::new();
    let attacker = Keypair::new();
    let reward_mint = Keypair::new();

    fund_user(&mut svm, &sender);
    fund_user(&mut svm, &recipient);
    fund_user(&mut svm, &attacker);

    let reward_mint_config = initialize_reward_mint(&mut svm, &admin, &reward_mint);
    let sender_ata = ensure_user_ata(&mut svm, &sender, reward_mint_config, reward_mint.pubkey());
    let recipient_ata = ensure_user_ata(
        &mut svm,
        &recipient,
        reward_mint_config,
        reward_mint.pubkey(),
    );
    mint_to_user(
        &mut svm,
        &admin,
        reward_mint_config,
        reward_mint.pubkey(),
        sender_ata,
    );

    assert!(!send_instruction(
        &mut svm,
        &attacker,
        transfer_reward_ix(
            attacker.pubkey(),
            reward_mint_config,
            reward_mint.pubkey(),
            sender_ata,
            recipient_ata,
            TRANSFER_AMOUNT
        )
    ));

    let sender_state = read_token_account(&svm, &sender_ata);
    let recipient_state = read_token_account(&svm, &recipient_ata);

    assert_eq!(sender_state.amount, MINT_AMOUNT);
    assert_eq!(recipient_state.amount, 0);
}
