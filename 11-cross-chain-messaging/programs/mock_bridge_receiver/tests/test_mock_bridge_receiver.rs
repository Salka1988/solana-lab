use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    mock_bridge_receiver::CrossChainMintMessage,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const SOURCE_CHAIN_ID: u16 = 2;
const PER_MESSAGE_LIMIT: u64 = 1_000;
const MESSAGE_AMOUNT: u64 = 500;

fn setup() -> (LiteSVM, Keypair) {
    let (mut svm, payer) = test_support::new_svm_with_payer();
    let bytes = include_bytes!("../../../target/deploy/mock_bridge_receiver.so");

    test_support::add_program(&mut svm, mock_bridge_receiver::id(), bytes);

    (svm, payer)
}

fn bridge_config_pda(registered_mint: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            mock_bridge_receiver::BRIDGE_CONFIG_SEED,
            registered_mint.as_ref(),
        ],
        &mock_bridge_receiver::id(),
    )
}

fn consumed_message_pda(bridge_config: Pubkey, source_chain_id: u16, nonce: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            mock_bridge_receiver::CONSUMED_MESSAGE_SEED,
            bridge_config.as_ref(),
            &source_chain_id.to_le_bytes(),
            &nonce.to_le_bytes(),
        ],
        &mock_bridge_receiver::id(),
    )
}

fn initialize_bridge_config_ix(
    admin: Pubkey,
    bridge_config: Pubkey,
    bridge_authority: Pubkey,
    registered_mint: Pubkey,
    per_message_limit: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        mock_bridge_receiver::id(),
        &mock_bridge_receiver::instruction::InitializeBridgeConfig { per_message_limit }.data(),
        mock_bridge_receiver::accounts::InitializeBridgeConfig {
            admin,
            bridge_config,
            bridge_authority,
            registered_mint,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn consume_cross_chain_mint_message_ix(
    bridge_authority: Pubkey,
    bridge_config: Pubkey,
    consumed_message: Pubkey,
    payer: Pubkey,
    message: CrossChainMintMessage,
) -> Instruction {
    Instruction::new_with_bytes(
        mock_bridge_receiver::id(),
        &mock_bridge_receiver::instruction::ConsumeCrossChainMintMessage { message }.data(),
        mock_bridge_receiver::accounts::ConsumeCrossChainMintMessage {
            bridge_authority,
            bridge_config,
            consumed_message,
            payer,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

struct Fixture {
    svm: LiteSVM,
    admin: Keypair,
    bridge_authority: Keypair,
    registered_mint: Pubkey,
    bridge_config: Pubkey,
}

fn initialized_fixture() -> Fixture {
    let (mut svm, admin) = setup();
    let bridge_authority = Keypair::new();
    let registered_mint = Keypair::new().pubkey();
    let (bridge_config, _) = bridge_config_pda(registered_mint);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_bridge_config_ix(
            admin.pubkey(),
            bridge_config,
            bridge_authority.pubkey(),
            registered_mint,
            PER_MESSAGE_LIMIT,
        ),
    ));

    Fixture {
        svm,
        admin,
        bridge_authority,
        registered_mint,
        bridge_config,
    }
}

fn valid_message(fixture: &Fixture, nonce: u64) -> CrossChainMintMessage {
    CrossChainMintMessage {
        source_chain_id: SOURCE_CHAIN_ID,
        destination_chain_id: mock_bridge_receiver::SOLANA_CHAIN_ID,
        nonce,
        mint: fixture.registered_mint,
        recipient: Keypair::new().pubkey(),
        amount: MESSAGE_AMOUNT,
    }
}

fn consume_message(
    fixture: &mut Fixture,
    message: CrossChainMintMessage,
) -> Result<(), litesvm::types::FailedTransactionMetadata> {
    let (consumed_message, _) = consumed_message_pda(
        fixture.bridge_config,
        message.source_chain_id,
        message.nonce,
    );

    test_support::send_instruction_with_signers_result(
        &mut fixture.svm,
        fixture.admin.pubkey(),
        consume_cross_chain_mint_message_ix(
            fixture.bridge_authority.pubkey(),
            fixture.bridge_config,
            consumed_message,
            fixture.admin.pubkey(),
            message,
        ),
        &[&fixture.admin, &fixture.bridge_authority],
    )
}

#[test]
fn initialize_bridge_config_creates_expected_state() {
    let (mut svm, admin) = setup();
    let bridge_authority = Keypair::new();
    let registered_mint = Keypair::new().pubkey();
    let (bridge_config, expected_bump) = bridge_config_pda(registered_mint);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_bridge_config_ix(
            admin.pubkey(),
            bridge_config,
            bridge_authority.pubkey(),
            registered_mint,
            PER_MESSAGE_LIMIT,
        ),
    ));

    let config = test_support::deserialize_account::<mock_bridge_receiver::BridgeConfig>(
        &svm,
        &bridge_config,
    );

    assert_eq!(config.admin, admin.pubkey());
    assert_eq!(config.bridge_authority, bridge_authority.pubkey());
    assert_eq!(config.registered_mint, registered_mint);
    assert_eq!(config.per_message_limit, PER_MESSAGE_LIMIT);
    assert_eq!(config.bump, expected_bump);
}

#[test]
fn consume_valid_message_records_nonce_receipt() {
    let mut fixture = initialized_fixture();
    let message = valid_message(&fixture, 7);
    let (consumed_message, expected_bump) = consumed_message_pda(
        fixture.bridge_config,
        message.source_chain_id,
        message.nonce,
    );

    assert!(consume_message(&mut fixture, message).is_ok());

    let consumed = test_support::deserialize_account::<mock_bridge_receiver::ConsumedMessage>(
        &fixture.svm,
        &consumed_message,
    );

    assert_eq!(consumed.bridge_config, fixture.bridge_config);
    assert_eq!(consumed.source_chain_id, SOURCE_CHAIN_ID);
    assert_eq!(consumed.nonce, 7);
    assert_eq!(consumed.mint, fixture.registered_mint);
    assert_eq!(consumed.recipient, message.recipient);
    assert_eq!(consumed.amount, MESSAGE_AMOUNT);
    assert_eq!(consumed.bump, expected_bump);
}

#[test]
fn replaying_same_source_chain_and_nonce_fails() {
    let mut fixture = initialized_fixture();
    let message = valid_message(&fixture, 42);

    assert!(consume_message(&mut fixture, message).is_ok());
    assert!(consume_message(&mut fixture, message).is_err());
}

#[test]
fn rejects_wrong_destination_chain() {
    let mut fixture = initialized_fixture();
    let mut message = valid_message(&fixture, 1);
    message.destination_chain_id = 99;

    let failure = consume_message(&mut fixture, message).unwrap_err();
    test_support::assert_failure_contains(&failure, "InvalidDestinationChain");
}

#[test]
fn rejects_unregistered_mint() {
    let mut fixture = initialized_fixture();
    let mut message = valid_message(&fixture, 1);
    message.mint = Keypair::new().pubkey();

    let failure = consume_message(&mut fixture, message).unwrap_err();
    test_support::assert_failure_contains(&failure, "UnregisteredMint");
}

#[test]
fn rejects_message_above_bridge_limit() {
    let mut fixture = initialized_fixture();
    let mut message = valid_message(&fixture, 1);
    message.amount = PER_MESSAGE_LIMIT + 1;

    let failure = consume_message(&mut fixture, message).unwrap_err();
    test_support::assert_failure_contains(&failure, "BridgeLimitExceeded");
}

#[test]
fn rejects_unauthorized_bridge_authority() {
    let mut fixture = initialized_fixture();
    let rogue_bridge_authority = Keypair::new();
    let message = valid_message(&fixture, 1);
    let (consumed_message, _) = consumed_message_pda(
        fixture.bridge_config,
        message.source_chain_id,
        message.nonce,
    );

    let failure = test_support::send_instruction_with_signers_result(
        &mut fixture.svm,
        fixture.admin.pubkey(),
        consume_cross_chain_mint_message_ix(
            rogue_bridge_authority.pubkey(),
            fixture.bridge_config,
            consumed_message,
            fixture.admin.pubkey(),
            message,
        ),
        &[&fixture.admin, &rogue_bridge_authority],
    )
    .unwrap_err();

    test_support::assert_failure_contains(&failure, "UnauthorizedBridgeAuthority");
}

#[test]
fn rejects_wrong_consumed_message_pda() {
    let mut fixture = initialized_fixture();
    let message = valid_message(&fixture, 1);
    let (wrong_consumed_message, _) =
        consumed_message_pda(fixture.bridge_config, message.source_chain_id, 999);

    let failure = test_support::send_instruction_with_signers_result(
        &mut fixture.svm,
        fixture.admin.pubkey(),
        consume_cross_chain_mint_message_ix(
            fixture.bridge_authority.pubkey(),
            fixture.bridge_config,
            wrong_consumed_message,
            fixture.admin.pubkey(),
            message,
        ),
        &[&fixture.admin, &fixture.bridge_authority],
    )
    .unwrap_err();

    test_support::assert_failure_contains(&failure, "ConstraintSeeds");
}
