use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::rent,
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::token::Mint,
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
            token_program: anchor_spl::token::ID,
            system_program: system_program::ID,
            rent: rent::ID,
        }
        .to_account_metas(None),
    )
}

fn send_instruction(
    svm: &mut LiteSVM,
    payer: &Keypair,
    reward_mint: &Keypair,
    instruction: Instruction,
) -> bool {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer, reward_mint])
        .unwrap();

    svm.send_transaction(tx).is_ok()
}

#[test]
fn initialize_reward_mint_creates_mint_with_pda_authority() {
    let (mut svm, admin) = setup();
    let reward_mint = Keypair::new();
    let (reward_mint_config, _) = reward_mint_config_pda();
    let (mint_authority, expected_mint_authority_bump) = mint_authority_pda();
    let instruction = initialize_reward_mint_ix(
        admin.pubkey(),
        reward_mint_config,
        mint_authority,
        reward_mint.pubkey(),
        REWARD_DECIMALS,
    );

    assert!(send_instruction(
        &mut svm,
        &admin,
        &reward_mint,
        instruction
    ));

    let config_account = svm
        .get_account(&reward_mint_config)
        .expect("reward mint config exists");
    let config_state =
        reward_token::RewardMintConfig::try_deserialize(&mut config_account.data.as_slice())
            .unwrap();

    assert_eq!(config_state.admin, admin.pubkey());
    assert_eq!(config_state.reward_mint, reward_mint.pubkey());
    assert_eq!(
        config_state.mint_authority_bump,
        expected_mint_authority_bump
    );
    assert_eq!(config_state.decimals, REWARD_DECIMALS);

    let mint_account = svm
        .get_account(&reward_mint.pubkey())
        .expect("reward mint exists");
    let mint_state = Mint::try_deserialize_unchecked(&mut mint_account.data.as_slice()).unwrap();

    assert_eq!(mint_state.decimals, REWARD_DECIMALS);
    assert_eq!(mint_state.supply, 0);
    assert_eq!(mint_state.mint_authority.unwrap(), mint_authority);
    assert_eq!(mint_state.freeze_authority.unwrap(), mint_authority);
}

#[test]
fn initialize_reward_mint_rejects_wrong_mint_authority_pda() {
    let (mut svm, admin) = setup();
    let reward_mint = Keypair::new();
    let (reward_mint_config, _) = reward_mint_config_pda();
    let wrong_mint_authority = Pubkey::new_unique();
    let instruction = initialize_reward_mint_ix(
        admin.pubkey(),
        reward_mint_config,
        wrong_mint_authority,
        reward_mint.pubkey(),
        REWARD_DECIMALS,
    );

    assert!(!send_instruction(
        &mut svm,
        &admin,
        &reward_mint,
        instruction
    ));
}
