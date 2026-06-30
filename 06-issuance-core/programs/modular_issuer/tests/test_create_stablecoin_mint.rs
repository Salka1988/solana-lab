use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::{
        spl_token_2022::{
            extension::permanent_delegate::PermanentDelegate,
            extension::{
                metadata_pointer::MetadataPointer, BaseStateWithExtensions, ExtensionType,
                StateWithExtensions,
            },
            state::Mint,
        },
        ID as TOKEN_2022_PROGRAM_ID,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
    spl_token_metadata_interface::state::TokenMetadata,
};

const GLOBAL_SUPPLY_CAP: u64 = 1_000_000_000_000;
const TOKEN_DECIMALS: u8 = 6;
const TOKEN_NAME: &str = "Lab Stablecoin";
const TOKEN_SYMBOL: &str = "LABUSD";
const TOKEN_URI: &str = "https://example.com/lab-usd.json";
const STANDALONE_TLV_STATE_HEADER_LEN: usize = 8;

fn setup() -> (LiteSVM, Keypair) {
    let (mut svm, payer) = test_support::new_svm_with_payer();
    let bytes = include_bytes!("../../../target/deploy/modular_issuer.so");

    test_support::add_program(&mut svm, modular_issuer::id(), bytes);

    (svm, payer)
}

fn protocol_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[modular_issuer::PROTOCOL_SEED], &modular_issuer::id())
}

fn mint_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[modular_issuer::STABLECOIN_MINT_SEED],
        &modular_issuer::id(),
    )
}

fn supply_stats_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[modular_issuer::SUPPLY_STATS_SEED], &modular_issuer::id())
}

fn mint_authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[modular_issuer::MINT_AUTHORITY_SEED],
        &modular_issuer::id(),
    )
}

fn initialize_protocol_ix(admin: Pubkey, protocol_config: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        modular_issuer::id(),
        &modular_issuer::instruction::InitializeProtocol {
            global_supply_cap: GLOBAL_SUPPLY_CAP,
        }
        .data(),
        modular_issuer::accounts::InitializeProtocol {
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
        modular_issuer::id(),
        &modular_issuer::instruction::CreateStablecoinMint {
            decimals: TOKEN_DECIMALS,
            name: TOKEN_NAME.to_string(),
            symbol: TOKEN_SYMBOL.to_string(),
            uri: TOKEN_URI.to_string(),
        }
        .data(),
        modular_issuer::accounts::CreateStablecoinMint {
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

fn initialize_protocol(svm: &mut LiteSVM, admin: &Keypair, protocol_config: Pubkey) {
    assert!(test_support::send_instruction(
        svm,
        admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config)
    ));
}

fn send_create_stablecoin_mint(
    svm: &mut LiteSVM,
    admin: &Keypair,
    mint: &Keypair,
    instruction: Instruction,
) -> bool {
    test_support::send_instruction_with_signers(svm, admin.pubkey(), instruction, &[admin, mint])
}

#[test]
fn create_stablecoin_mint_initializes_token_2022_mint_config_and_supply_stats() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let (protocol_config, _) = protocol_config_pda();
    let (mint_config, expected_mint_config_bump) = mint_config_pda();
    let (supply_stats, expected_supply_stats_bump) = supply_stats_pda();
    let (mint_authority, expected_mint_authority_bump) = mint_authority_pda();

    initialize_protocol(&mut svm, &admin, protocol_config);

    assert!(send_create_stablecoin_mint(
        &mut svm,
        &admin,
        &mint,
        create_stablecoin_mint_ix(
            admin.pubkey(),
            protocol_config,
            mint_config,
            supply_stats,
            mint_authority,
            mint.pubkey(),
        ),
    ));

    let mint_account = svm.get_account(&mint.pubkey()).expect("mint exists");
    let mint_state = StateWithExtensions::<Mint>::unpack(&mint_account.data).unwrap();
    let metadata_pointer = mint_state.get_extension::<MetadataPointer>().unwrap();
    let permanent_delegate = mint_state.get_extension::<PermanentDelegate>().unwrap();
    let token_metadata = mint_state
        .get_variable_len_extension::<TokenMetadata>()
        .unwrap();
    let expected_metadata = TokenMetadata {
        mint: mint.pubkey(),
        name: TOKEN_NAME.to_string(),
        symbol: TOKEN_SYMBOL.to_string(),
        uri: TOKEN_URI.to_string(),
        ..Default::default()
    };
    let expected_mint_len = ExtensionType::try_calculate_account_len::<Mint>(&[
        ExtensionType::MetadataPointer,
        ExtensionType::PermanentDelegate,
    ])
    .unwrap()
        + expected_metadata.tlv_size_of().unwrap()
        - STANDALONE_TLV_STATE_HEADER_LEN;
    let protocol_state =
        test_support::deserialize_account::<modular_issuer::ProtocolConfig>(&svm, &protocol_config);
    let mint_config_state = test_support::deserialize_account::<modular_issuer::StablecoinMintConfig>(
        &svm,
        &mint_config,
    );
    let supply_stats_state =
        test_support::deserialize_account::<modular_issuer::GlobalSupplyStats>(&svm, &supply_stats);

    assert_eq!(mint_account.owner, TOKEN_2022_PROGRAM_ID);
    assert_eq!(mint_account.data.len(), expected_mint_len);
    assert_eq!(mint_state.base.decimals, TOKEN_DECIMALS);
    assert_eq!(mint_state.base.supply, 0);
    assert_eq!(mint_state.base.mint_authority, Some(mint_authority).into());
    assert_eq!(
        mint_state.base.freeze_authority,
        Some(mint_authority).into()
    );
    assert_eq!(metadata_pointer.metadata_address.0, mint.pubkey());
    assert_eq!(metadata_pointer.authority.0, mint_authority);
    assert_eq!(permanent_delegate.delegate.0, mint_authority);
    assert_eq!(token_metadata.mint, mint.pubkey());
    assert_eq!(token_metadata.name, TOKEN_NAME);
    assert_eq!(token_metadata.symbol, TOKEN_SYMBOL);
    assert_eq!(token_metadata.uri, TOKEN_URI);
    assert_eq!(protocol_state.stablecoin_mint, mint.pubkey());
    assert_eq!(mint_config_state.protocol_config, protocol_config);
    assert_eq!(mint_config_state.mint, mint.pubkey());
    assert_eq!(mint_config_state.mint_authority, mint_authority);
    assert_eq!(mint_config_state.supply_stats, supply_stats);
    assert_eq!(mint_config_state.decimals, TOKEN_DECIMALS);
    assert_eq!(
        mint_config_state.mint_authority_bump,
        expected_mint_authority_bump
    );
    assert_eq!(mint_config_state.bump, expected_mint_config_bump);
    assert_eq!(supply_stats_state.protocol_config, protocol_config);
    assert_eq!(supply_stats_state.mint, mint.pubkey());
    assert_eq!(supply_stats_state.current_supply, 0);
    assert_eq!(supply_stats_state.total_minted, 0);
    assert_eq!(supply_stats_state.total_burned, 0);
    assert_eq!(supply_stats_state.bump, expected_supply_stats_bump);
}

#[test]
fn create_stablecoin_mint_rejects_non_admin() {
    let (mut svm, admin) = setup();
    let attacker = Keypair::new();
    let mint = Keypair::new();
    let (protocol_config, _) = protocol_config_pda();
    let (mint_config, _) = mint_config_pda();
    let (supply_stats, _) = supply_stats_pda();
    let (mint_authority, _) = mint_authority_pda();

    test_support::fund_user(&mut svm, &attacker);
    initialize_protocol(&mut svm, &admin, protocol_config);

    let result = test_support::send_instruction_with_signers_result(
        &mut svm,
        attacker.pubkey(),
        create_stablecoin_mint_ix(
            attacker.pubkey(),
            protocol_config,
            mint_config,
            supply_stats,
            mint_authority,
            mint.pubkey(),
        ),
        &[&attacker, &mint],
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "ConstraintHasOne");
}

#[test]
fn create_stablecoin_mint_rejects_wrong_mint_authority_pda() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let (protocol_config, _) = protocol_config_pda();
    let (mint_config, _) = mint_config_pda();
    let (supply_stats, _) = supply_stats_pda();
    let wrong_mint_authority = Keypair::new().pubkey();

    initialize_protocol(&mut svm, &admin, protocol_config);

    let result = test_support::send_instruction_with_signers_result(
        &mut svm,
        admin.pubkey(),
        create_stablecoin_mint_ix(
            admin.pubkey(),
            protocol_config,
            mint_config,
            supply_stats,
            wrong_mint_authority,
            mint.pubkey(),
        ),
        &[&admin, &mint],
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "ConstraintSeeds");
}

#[test]
fn create_stablecoin_mint_rejects_duplicate_global_mint() {
    let (mut svm, admin) = setup();
    let first_mint = Keypair::new();
    let second_mint = Keypair::new();
    let (protocol_config, _) = protocol_config_pda();
    let (mint_config, _) = mint_config_pda();
    let (supply_stats, _) = supply_stats_pda();
    let (mint_authority, _) = mint_authority_pda();

    initialize_protocol(&mut svm, &admin, protocol_config);

    assert!(send_create_stablecoin_mint(
        &mut svm,
        &admin,
        &first_mint,
        create_stablecoin_mint_ix(
            admin.pubkey(),
            protocol_config,
            mint_config,
            supply_stats,
            mint_authority,
            first_mint.pubkey(),
        ),
    ));

    let result = test_support::send_instruction_with_signers_result(
        &mut svm,
        admin.pubkey(),
        create_stablecoin_mint_ix(
            admin.pubkey(),
            protocol_config,
            mint_config,
            supply_stats,
            mint_authority,
            second_mint.pubkey(),
        ),
        &[&admin, &second_mint],
    );

    test_support::assert_failure_contains(&result.unwrap_err(), "already in use");
}
