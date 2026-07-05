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

fn setup() -> (LiteSVM, Keypair) {
    test_support::new_svm_with_program(
        reward_token::id(),
        include_bytes!("../../../target/deploy/reward_token.so"),
    )
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

#[test]
fn ensure_user_ata_creates_expected_ata() {
    let (mut svm, admin) = setup();
    let user = Keypair::new();
    let reward_mint = Keypair::new();

    fund_user(&mut svm, &user);

    let reward_mint_config = initialize_reward_mint(&mut svm, &admin, &reward_mint);
    let user_ata = get_associated_token_address(&user.pubkey(), &reward_mint.pubkey());

    assert!(send_instruction(
        &mut svm,
        &user,
        ensure_user_ata_ix(
            user.pubkey(),
            reward_mint_config,
            reward_mint.pubkey(),
            user_ata
        )
    ));

    let ata_account = svm.get_account(&user_ata).expect("user ATA exists");
    let token_state =
        TokenAccount::try_deserialize_unchecked(&mut ata_account.data.as_slice()).unwrap();

    assert_eq!(token_state.owner, user.pubkey());
    assert_eq!(token_state.mint, reward_mint.pubkey());
    assert_eq!(token_state.amount, 0);
}

#[test]
fn ensure_user_ata_is_idempotent() {
    let (mut svm, admin) = setup();
    let user = Keypair::new();
    let reward_mint = Keypair::new();

    fund_user(&mut svm, &user);

    let reward_mint_config = initialize_reward_mint(&mut svm, &admin, &reward_mint);
    let user_ata = get_associated_token_address(&user.pubkey(), &reward_mint.pubkey());

    assert!(send_instruction(
        &mut svm,
        &user,
        ensure_user_ata_ix(
            user.pubkey(),
            reward_mint_config,
            reward_mint.pubkey(),
            user_ata
        )
    ));
    svm.expire_blockhash();
    assert!(send_instruction(
        &mut svm,
        &user,
        ensure_user_ata_ix(
            user.pubkey(),
            reward_mint_config,
            reward_mint.pubkey(),
            user_ata
        )
    ));
}

#[test]
fn ensure_user_ata_rejects_wrong_ata() {
    let (mut svm, admin) = setup();
    let user = Keypair::new();
    let reward_mint = Keypair::new();
    let wrong_ata = Pubkey::new_unique();

    fund_user(&mut svm, &user);

    let reward_mint_config = initialize_reward_mint(&mut svm, &admin, &reward_mint);

    assert!(!send_instruction(
        &mut svm,
        &user,
        ensure_user_ata_ix(
            user.pubkey(),
            reward_mint_config,
            reward_mint.pubkey(),
            wrong_ata
        )
    ));
}
