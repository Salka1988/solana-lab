use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{
            instruction::{AccountMeta, Instruction},
            system_program,
        },
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::ID as TOKEN_2022_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
    spl_transfer_hook_interface::instruction::TransferHookInstruction,
    stablecoin_bridge_receiver::CrossChainMintMessage,
};

const GLOBAL_SUPPLY_CAP: u64 = 1_000_000;
const ISSUER_MINT_LIMIT: u64 = 500_000;
const MINT_AMOUNT: u64 = 100_000;
const REDEMPTION_AMOUNT: u64 = 25_000;
const BRIDGE_AMOUNT: u64 = 50_000;
const TOKEN_DECIMALS: u8 = 6;
const TOKEN_NAME: &str = "M0 Lab Stablecoin";
const TOKEN_SYMBOL: &str = "M0USD";
const TOKEN_URI: &str = "https://example.com/m0-usd.json";
const MAX_TRANSFER_AMOUNT: u64 = 200_000;
const DAILY_TRANSFER_LIMIT: u64 = 300_000;
const SOURCE_CHAIN_ID: u16 = 2;

fn setup() -> (LiteSVM, Keypair) {
    test_support::new_svm_with_programs(&[
        (
            stablecoin_issuer::id(),
            include_bytes!("../../../target/deploy/stablecoin_issuer.so"),
        ),
        (
            stablecoin_compliance_hook::id(),
            include_bytes!("../../../target/deploy/stablecoin_compliance_hook.so"),
        ),
        (
            stablecoin_redemption::id(),
            include_bytes!("../../../target/deploy/stablecoin_redemption.so"),
        ),
        (
            stablecoin_bridge_receiver::id(),
            include_bytes!("../../../target/deploy/stablecoin_bridge_receiver.so"),
        ),
    ])
}

fn send_with_signers(
    svm: &mut LiteSVM,
    fee_payer: Pubkey,
    instruction: Instruction,
    signers: &[&Keypair],
) -> Result<(), litesvm::types::FailedTransactionMetadata> {
    test_support::send_instruction_with_signers_result(svm, fee_payer, instruction, signers)
}

fn issuer_protocol_config_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::PROTOCOL_SEED],
        &stablecoin_issuer::id(),
    )
    .0
}

fn issuer_mint_config_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::STABLECOIN_MINT_SEED],
        &stablecoin_issuer::id(),
    )
    .0
}

fn issuer_supply_stats_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::SUPPLY_STATS_SEED],
        &stablecoin_issuer::id(),
    )
    .0
}

fn issuer_mint_authority_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::MINT_AUTHORITY_SEED],
        &stablecoin_issuer::id(),
    )
    .0
}

fn issuer_config_pda(authority: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::ISSUER_SEED, authority.as_ref()],
        &stablecoin_issuer::id(),
    )
    .0
}

fn issuer_stats_pda(authority: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::ISSUER_STATS_SEED, authority.as_ref()],
        &stablecoin_issuer::id(),
    )
    .0
}

fn initialize_issuer_protocol_ix(admin: Pubkey, protocol_config: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::InitializeProtocol {
            global_supply_cap: GLOBAL_SUPPLY_CAP,
        }
        .data(),
        stablecoin_issuer::accounts::InitializeProtocol {
            admin,
            protocol_config,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn create_stablecoin_mint_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    mint_config: Pubkey,
    supply_stats: Pubkey,
    mint_authority: Pubkey,
    mint: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::CreateStablecoinMint {
            decimals: TOKEN_DECIMALS,
            name: TOKEN_NAME.to_string(),
            symbol: TOKEN_SYMBOL.to_string(),
            uri: TOKEN_URI.to_string(),
        }
        .data(),
        stablecoin_issuer::accounts::CreateStablecoinMint {
            admin,
            protocol_config,
            mint_config,
            supply_stats,
            mint_authority,
            mint,
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn register_issuer_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    issuer_config: Pubkey,
    issuer_stats: Pubkey,
    issuer_authority: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::RegisterIssuer {
            mint_limit: ISSUER_MINT_LIMIT,
        }
        .data(),
        stablecoin_issuer::accounts::RegisterIssuer {
            admin,
            protocol_config,
            issuer_config,
            issuer_stats,
            issuer_authority,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

#[allow(clippy::too_many_arguments)]
fn mint_to_user_ix(
    issuer_authority: Pubkey,
    protocol_config: Pubkey,
    mint_config: Pubkey,
    supply_stats: Pubkey,
    issuer_config: Pubkey,
    issuer_stats: Pubkey,
    mint_authority: Pubkey,
    mint: Pubkey,
    user: Pubkey,
    user_token_account: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::MintToUser {
            amount: MINT_AMOUNT,
        }
        .data(),
        stablecoin_issuer::accounts::MintToUser {
            issuer_authority,
            protocol_config,
            mint_config,
            supply_stats,
            issuer_config,
            issuer_stats,
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

fn compliance_config_pda(mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            stablecoin_compliance_hook::COMPLIANCE_CONFIG_SEED,
            mint.as_ref(),
        ],
        &stablecoin_compliance_hook::id(),
    )
    .0
}

fn user_compliance_pda(mint: Pubkey, user: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            stablecoin_compliance_hook::USER_COMPLIANCE_SEED,
            mint.as_ref(),
            user.as_ref(),
        ],
        &stablecoin_compliance_hook::id(),
    )
    .0
}

fn extra_account_meta_list_pda(mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            stablecoin_compliance_hook::EXTRA_ACCOUNT_METAS_SEED,
            mint.as_ref(),
        ],
        &stablecoin_compliance_hook::id(),
    )
    .0
}

fn initialize_compliance_config_ix(admin: Pubkey, config: Pubkey, mint: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_compliance_hook::id(),
        &stablecoin_compliance_hook::instruction::InitializeComplianceConfig {
            max_transfer_amount: MAX_TRANSFER_AMOUNT,
            daily_transfer_limit: DAILY_TRANSFER_LIMIT,
        }
        .data(),
        stablecoin_compliance_hook::accounts::InitializeComplianceConfig {
            admin,
            config,
            mint,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn initialize_extra_account_meta_list_ix(
    admin: Pubkey,
    config: Pubkey,
    extra_account_meta_list: Pubkey,
    mint: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_compliance_hook::id(),
        &stablecoin_compliance_hook::instruction::InitializeExtraAccountMetaList {}.data(),
        stablecoin_compliance_hook::accounts::InitializeExtraAccountMetaList {
            admin,
            config,
            extra_account_meta_list,
            mint,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn set_user_compliance_ix(
    admin: Pubkey,
    config: Pubkey,
    user_compliance: Pubkey,
    user: Pubkey,
    mint: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_compliance_hook::id(),
        &stablecoin_compliance_hook::instruction::SetUserCompliance {
            allowlisted: true,
            blocked: false,
            issuer_active: true,
        }
        .data(),
        stablecoin_compliance_hook::accounts::SetUserCompliance {
            admin,
            config,
            user_compliance,
            user,
            mint,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn execute_hook_ix(
    source: Pubkey,
    mint: Pubkey,
    destination: Pubkey,
    authority: Pubkey,
    config: Pubkey,
    source_compliance: Pubkey,
    destination_compliance: Pubkey,
) -> Instruction {
    Instruction {
        program_id: stablecoin_compliance_hook::id(),
        accounts: vec![
            AccountMeta::new_readonly(source, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(destination, false),
            AccountMeta::new_readonly(authority, false),
            AccountMeta::new_readonly(extra_account_meta_list_pda(mint), false),
            AccountMeta::new_readonly(config, false),
            AccountMeta::new(source_compliance, false),
            AccountMeta::new_readonly(destination_compliance, false),
        ],
        data: TransferHookInstruction::Execute {
            amount: MINT_AMOUNT / 2,
        }
        .pack(),
    }
}

fn redemption_protocol_config_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_redemption::PROTOCOL_SEED],
        &stablecoin_redemption::id(),
    )
    .0
}

fn redemption_vault_pda(protocol_config: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            stablecoin_redemption::REDEMPTION_VAULT_SEED,
            protocol_config.as_ref(),
        ],
        &stablecoin_redemption::id(),
    )
    .0
}

fn redemption_request_pda(protocol_config: Pubkey, request_id: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            stablecoin_redemption::REDEMPTION_REQUEST_SEED,
            protocol_config.as_ref(),
            &request_id.to_le_bytes(),
        ],
        &stablecoin_redemption::id(),
    )
    .0
}

fn admin_action_log_pda(protocol_config: Pubkey, action_id: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            stablecoin_redemption::ADMIN_ACTION_LOG_SEED,
            protocol_config.as_ref(),
            &action_id.to_le_bytes(),
        ],
        &stablecoin_redemption::id(),
    )
    .0
}

fn initialize_redemption_protocol_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    redemption_vault: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_redemption::id(),
        &stablecoin_redemption::instruction::InitializeProtocol {}.data(),
        stablecoin_redemption::accounts::InitializeProtocol {
            admin,
            protocol_config,
            redemption_vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn request_redemption_ix(
    owner: Pubkey,
    protocol_config: Pubkey,
    redemption_vault: Pubkey,
    redemption_request: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_redemption::id(),
        &stablecoin_redemption::instruction::RequestRedemption {
            amount: REDEMPTION_AMOUNT,
        }
        .data(),
        stablecoin_redemption::accounts::RequestRedemption {
            owner,
            protocol_config,
            redemption_vault,
            redemption_request,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn complete_redemption_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    redemption_vault: Pubkey,
    redemption_request: Pubkey,
    admin_action_log: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_redemption::id(),
        &stablecoin_redemption::instruction::CompleteRedemption {}.data(),
        stablecoin_redemption::accounts::AdminRedemptionAction {
            admin,
            protocol_config,
            redemption_vault,
            redemption_request,
            admin_action_log,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn bridge_config_pda(registered_mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            stablecoin_bridge_receiver::BRIDGE_CONFIG_SEED,
            registered_mint.as_ref(),
        ],
        &stablecoin_bridge_receiver::id(),
    )
    .0
}

fn consumed_message_pda(bridge_config: Pubkey, source_chain_id: u16, nonce: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            stablecoin_bridge_receiver::CONSUMED_MESSAGE_SEED,
            bridge_config.as_ref(),
            &source_chain_id.to_le_bytes(),
            &nonce.to_le_bytes(),
        ],
        &stablecoin_bridge_receiver::id(),
    )
    .0
}

fn initialize_bridge_config_ix(
    admin: Pubkey,
    bridge_config: Pubkey,
    bridge_authority: Pubkey,
    registered_mint: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_bridge_receiver::id(),
        &stablecoin_bridge_receiver::instruction::InitializeBridgeConfig {
            per_message_limit: BRIDGE_AMOUNT,
        }
        .data(),
        stablecoin_bridge_receiver::accounts::InitializeBridgeConfig {
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
        stablecoin_bridge_receiver::id(),
        &stablecoin_bridge_receiver::instruction::ConsumeCrossChainMintMessage { message }.data(),
        stablecoin_bridge_receiver::accounts::ConsumeCrossChainMintMessage {
            bridge_authority,
            bridge_config,
            consumed_message,
            payer,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

#[test]
fn stablecoin_protocol_lifecycle_across_programs() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let issuer_authority = Keypair::new();
    let user = Keypair::new();
    let user_token_account = Keypair::new();
    let destination_token_account = Keypair::new().pubkey();
    let transfer_authority = Keypair::new().pubkey();

    test_support::fund_user(&mut svm, &issuer_authority);
    test_support::fund_user(&mut svm, &user);

    let issuer_protocol_config = issuer_protocol_config_pda();
    let mint_config = issuer_mint_config_pda();
    let supply_stats = issuer_supply_stats_pda();
    let mint_authority = issuer_mint_authority_pda();
    let issuer_config = issuer_config_pda(issuer_authority.pubkey());
    let issuer_stats = issuer_stats_pda(issuer_authority.pubkey());

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_issuer_protocol_ix(admin.pubkey(), issuer_protocol_config),
    ));
    assert!(send_with_signers(
        &mut svm,
        admin.pubkey(),
        create_stablecoin_mint_ix(
            admin.pubkey(),
            issuer_protocol_config,
            mint_config,
            supply_stats,
            mint_authority,
            mint.pubkey(),
        ),
        &[&admin, &mint],
    )
    .is_ok());
    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        register_issuer_ix(
            admin.pubkey(),
            issuer_protocol_config,
            issuer_config,
            issuer_stats,
            issuer_authority.pubkey(),
        ),
    ));
    assert!(send_with_signers(
        &mut svm,
        issuer_authority.pubkey(),
        mint_to_user_ix(
            issuer_authority.pubkey(),
            issuer_protocol_config,
            mint_config,
            supply_stats,
            issuer_config,
            issuer_stats,
            mint_authority,
            mint.pubkey(),
            user.pubkey(),
            user_token_account.pubkey(),
        ),
        &[&issuer_authority, &user_token_account],
    )
    .is_ok());

    let issuer_supply = test_support::deserialize_account::<stablecoin_issuer::GlobalSupplyStats>(
        &svm,
        &supply_stats,
    );
    assert_eq!(issuer_supply.current_supply, MINT_AMOUNT);

    let compliance_config = compliance_config_pda(mint.pubkey());
    let source_compliance = user_compliance_pda(mint.pubkey(), user_token_account.pubkey());
    let destination_compliance = user_compliance_pda(mint.pubkey(), destination_token_account);
    let extra_account_meta_list = extra_account_meta_list_pda(mint.pubkey());

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_compliance_config_ix(admin.pubkey(), compliance_config, mint.pubkey()),
    ));
    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_extra_account_meta_list_ix(
            admin.pubkey(),
            compliance_config,
            extra_account_meta_list,
            mint.pubkey(),
        ),
    ));
    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        set_user_compliance_ix(
            admin.pubkey(),
            compliance_config,
            source_compliance,
            user_token_account.pubkey(),
            mint.pubkey(),
        ),
    ));
    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        set_user_compliance_ix(
            admin.pubkey(),
            compliance_config,
            destination_compliance,
            destination_token_account,
            mint.pubkey(),
        ),
    ));
    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        execute_hook_ix(
            user_token_account.pubkey(),
            mint.pubkey(),
            destination_token_account,
            transfer_authority,
            compliance_config,
            source_compliance,
            destination_compliance,
        ),
    ));

    let source_profile = test_support::deserialize_account::<
        stablecoin_compliance_hook::UserCompliance,
    >(&svm, &source_compliance);
    assert_eq!(source_profile.transferred_today, MINT_AMOUNT / 2);

    let redemption_protocol_config = redemption_protocol_config_pda();
    let redemption_vault = redemption_vault_pda(redemption_protocol_config);
    let redemption_request = redemption_request_pda(redemption_protocol_config, 0);
    let admin_action_log = admin_action_log_pda(redemption_protocol_config, 0);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_redemption_protocol_ix(
            admin.pubkey(),
            redemption_protocol_config,
            redemption_vault,
        ),
    ));
    assert!(test_support::send_instruction(
        &mut svm,
        &user,
        request_redemption_ix(
            user.pubkey(),
            redemption_protocol_config,
            redemption_vault,
            redemption_request,
        ),
    ));
    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        complete_redemption_ix(
            admin.pubkey(),
            redemption_protocol_config,
            redemption_vault,
            redemption_request,
            admin_action_log,
        ),
    ));

    let redemption_request_state = test_support::deserialize_account::<
        stablecoin_redemption::RedemptionRequest,
    >(&svm, &redemption_request);
    assert_eq!(
        redemption_request_state.status,
        stablecoin_redemption::RedemptionStatus::Completed
    );

    let bridge_authority = Keypair::new();
    let bridge_config = bridge_config_pda(mint.pubkey());
    let nonce = 7;
    let consumed_message = consumed_message_pda(bridge_config, SOURCE_CHAIN_ID, nonce);
    let bridge_message = CrossChainMintMessage {
        source_chain_id: SOURCE_CHAIN_ID,
        destination_chain_id: stablecoin_bridge_receiver::SOLANA_CHAIN_ID,
        nonce,
        mint: mint.pubkey(),
        recipient: user.pubkey(),
        amount: BRIDGE_AMOUNT,
    };

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_bridge_config_ix(
            admin.pubkey(),
            bridge_config,
            bridge_authority.pubkey(),
            mint.pubkey(),
        ),
    ));
    assert!(send_with_signers(
        &mut svm,
        admin.pubkey(),
        consume_cross_chain_mint_message_ix(
            bridge_authority.pubkey(),
            bridge_config,
            consumed_message,
            admin.pubkey(),
            bridge_message,
        ),
        &[&admin, &bridge_authority],
    )
    .is_ok());

    let replay = send_with_signers(
        &mut svm,
        admin.pubkey(),
        consume_cross_chain_mint_message_ix(
            bridge_authority.pubkey(),
            bridge_config,
            consumed_message,
            admin.pubkey(),
            bridge_message,
        ),
        &[&admin, &bridge_authority],
    );
    test_support::assert_result_fails_with(replay, "AlreadyProcessed");
}
