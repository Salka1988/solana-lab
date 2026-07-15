use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

fn setup() -> (LiteSVM, Keypair) {
    test_support::new_svm_with_program(
        spot_settlement::id(),
        include_bytes!("../../../target/deploy/spot_settlement.so"),
    )
}

fn protocol_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[spot_settlement::PROTOCOL_CONFIG_SEED],
        &spot_settlement::id(),
    )
}

fn market_config_pda(base_mint: Pubkey, quote_mint: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            spot_settlement::MARKET_CONFIG_SEED,
            base_mint.as_ref(),
            quote_mint.as_ref(),
        ],
        &spot_settlement::id(),
    )
}

fn vault_authority_pda(market_config: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            spot_settlement::VAULT_AUTHORITY_SEED,
            market_config.as_ref(),
        ],
        &spot_settlement::id(),
    )
}

fn initialize_protocol_ix(admin: Pubkey, protocol_config: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        spot_settlement::id(),
        &spot_settlement::instruction::InitializeProtocol {}.data(),
        spot_settlement::accounts::InitializeProtocol {
            admin,
            protocol_config,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

struct MarketAccounts {
    protocol_config: Pubkey,
    market_config: Pubkey,
    base_mint: Pubkey,
    quote_mint: Pubkey,
    base_vault: Pubkey,
    quote_vault: Pubkey,
    vault_authority: Pubkey,
    settlement_authority: Pubkey,
}

fn initialize_market_ix(admin: Pubkey, accounts: &MarketAccounts) -> Instruction {
    Instruction::new_with_bytes(
        spot_settlement::id(),
        &spot_settlement::instruction::InitializeMarket {}.data(),
        spot_settlement::accounts::InitializeMarket {
            admin,
            protocol_config: accounts.protocol_config,
            market_config: accounts.market_config,
            base_mint: accounts.base_mint,
            quote_mint: accounts.quote_mint,
            base_vault: accounts.base_vault,
            quote_vault: accounts.quote_vault,
            vault_authority: accounts.vault_authority,
            settlement_authority: accounts.settlement_authority,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn initialized_protocol() -> (LiteSVM, Keypair, Pubkey) {
    let (mut svm, admin) = setup();
    let (protocol_config, _) = protocol_config_pda();

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config),
    ));

    (svm, admin, protocol_config)
}

fn market_accounts(protocol_config: Pubkey) -> MarketAccounts {
    let base_mint = Keypair::new().pubkey();
    let quote_mint = Keypair::new().pubkey();
    let (market_config, _) = market_config_pda(base_mint, quote_mint);
    let (vault_authority, _) = vault_authority_pda(market_config);

    MarketAccounts {
        protocol_config,
        market_config,
        base_mint,
        quote_mint,
        base_vault: Keypair::new().pubkey(),
        quote_vault: Keypair::new().pubkey(),
        vault_authority,
        settlement_authority: Keypair::new().pubkey(),
    }
}

#[test]
fn initialize_protocol_creates_expected_state() {
    let (mut svm, admin) = setup();
    let (protocol_config, expected_bump) = protocol_config_pda();

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config),
    ));

    let config = test_support::deserialize_account::<spot_settlement::ProtocolConfig>(
        &svm,
        &protocol_config,
    );

    assert_eq!(config.admin, admin.pubkey());
    assert_eq!(config.bump, expected_bump);
}

#[test]
fn initialize_market_creates_expected_state() {
    let (mut svm, admin, protocol_config) = initialized_protocol();
    let accounts = market_accounts(protocol_config);
    let (_, expected_market_bump) = market_config_pda(accounts.base_mint, accounts.quote_mint);
    let (_, expected_vault_authority_bump) = vault_authority_pda(accounts.market_config);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        initialize_market_ix(admin.pubkey(), &accounts),
    ));

    let market = test_support::deserialize_account::<spot_settlement::MarketConfig>(
        &svm,
        &accounts.market_config,
    );

    assert_eq!(market.protocol_config, accounts.protocol_config);
    assert_eq!(market.admin, admin.pubkey());
    assert_eq!(market.settlement_authority, accounts.settlement_authority);
    assert_eq!(market.base_mint, accounts.base_mint);
    assert_eq!(market.quote_mint, accounts.quote_mint);
    assert_eq!(market.base_vault, accounts.base_vault);
    assert_eq!(market.quote_vault, accounts.quote_vault);
    assert_eq!(market.vault_authority, accounts.vault_authority);
    assert_eq!(market.vault_authority_bump, expected_vault_authority_bump);
    assert!(!market.paused);
    assert_eq!(market.bump, expected_market_bump);
}

#[test]
fn initialize_market_rejects_same_mints() {
    let (mut svm, admin, protocol_config) = initialized_protocol();
    let mut accounts = market_accounts(protocol_config);
    accounts.quote_mint = accounts.base_mint;
    let (market_config, _) = market_config_pda(accounts.base_mint, accounts.quote_mint);
    let (vault_authority, _) = vault_authority_pda(market_config);
    accounts.market_config = market_config;
    accounts.vault_authority = vault_authority;

    test_support::assert_result_fails_with(
        test_support::send_instruction_result(
            &mut svm,
            &admin,
            initialize_market_ix(admin.pubkey(), &accounts),
        ),
        "Market base and quote mints must differ",
    );
}

#[test]
fn initialize_market_rejects_same_vaults() {
    let (mut svm, admin, protocol_config) = initialized_protocol();
    let mut accounts = market_accounts(protocol_config);
    accounts.quote_vault = accounts.base_vault;

    test_support::assert_result_fails_with(
        test_support::send_instruction_result(
            &mut svm,
            &admin,
            initialize_market_ix(admin.pubkey(), &accounts),
        ),
        "Market vaults must differ",
    );
}
