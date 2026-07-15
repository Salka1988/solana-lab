use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{
            instruction::Instruction, program_pack::Pack, system_instruction, system_program,
        },
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::{
        spl_token_2022::{
            instruction as token_2022_instruction,
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

const DECIMALS: u8 = 6;
const INITIAL_BASE_AMOUNT: u64 = 1_000_000;
const DEPOSIT_AMOUNT: u64 = 250_000;

fn setup() -> (LiteSVM, Keypair) {
    test_support::new_svm_with_program(
        spot_settlement::id(),
        include_bytes!("../../../target/deploy/spot_settlement.so"),
    )
}

fn protocol_config_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[spot_settlement::PROTOCOL_CONFIG_SEED],
        &spot_settlement::id(),
    )
    .0
}

fn market_config_pda(base_mint: Pubkey, quote_mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            spot_settlement::MARKET_CONFIG_SEED,
            base_mint.as_ref(),
            quote_mint.as_ref(),
        ],
        &spot_settlement::id(),
    )
    .0
}

fn trader_balance_pda(market_config: Pubkey, trader: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            spot_settlement::TRADER_MARKET_BALANCE_SEED,
            market_config.as_ref(),
            trader.as_ref(),
        ],
        &spot_settlement::id(),
    )
    .0
}

fn vault_authority_pda(market_config: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            spot_settlement::VAULT_AUTHORITY_SEED,
            market_config.as_ref(),
        ],
        &spot_settlement::id(),
    )
    .0
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

struct DepositParams {
    trader: Pubkey,
    market_config: Pubkey,
    trader_balance: Pubkey,
    source: Pubkey,
    vault: Pubkey,
    mint: Pubkey,
    asset: spot_settlement::CustodyAsset,
    amount: u64,
}

struct WithdrawParams {
    trader: Pubkey,
    market_config: Pubkey,
    trader_balance: Pubkey,
    vault: Pubkey,
    destination: Pubkey,
    mint: Pubkey,
    vault_authority: Pubkey,
    asset: spot_settlement::CustodyAsset,
    amount: u64,
}

fn withdraw_ix(params: WithdrawParams) -> Instruction {
    Instruction::new_with_bytes(
        spot_settlement::id(),
        &spot_settlement::instruction::Withdraw {
            asset: params.asset,
            amount: params.amount,
        }
        .data(),
        spot_settlement::accounts::Withdraw {
            trader: params.trader,
            market_config: params.market_config,
            trader_balance: params.trader_balance,
            vault: params.vault,
            destination: params.destination,
            mint: params.mint,
            vault_authority: params.vault_authority,
            token_program: TOKEN_2022_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}

fn deposit_ix(params: DepositParams) -> Instruction {
    Instruction::new_with_bytes(
        spot_settlement::id(),
        &spot_settlement::instruction::Deposit {
            asset: params.asset,
            amount: params.amount,
        }
        .data(),
        spot_settlement::accounts::Deposit {
            trader: params.trader,
            market_config: params.market_config,
            trader_balance: params.trader_balance,
            source: params.source,
            vault: params.vault,
            mint: params.mint,
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn send_setup_transaction(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instructions: Vec<Instruction>,
    signers: &[&Keypair],
) {
    let blockhash = svm.latest_blockhash();
    let message = Message::new_with_blockhash(&instructions, Some(&payer.pubkey()), &blockhash);
    let required_signer_keys =
        &message.account_keys[..message.header.num_required_signatures as usize];
    let mut required_signers = Vec::new();
    for signer in signers
        .iter()
        .copied()
        .filter(|signer| required_signer_keys.contains(&signer.pubkey()))
    {
        if !required_signers
            .iter()
            .any(|existing: &&Keypair| existing.pubkey() == signer.pubkey())
        {
            required_signers.push(signer);
        }
    }
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(message), &required_signers)
        .unwrap();

    svm.send_transaction(tx).unwrap();
}

fn create_mint(svm: &mut LiteSVM, payer: &Keypair, mint: &Keypair, authority: Pubkey) {
    let rent = svm.minimum_balance_for_rent_exemption(Mint::LEN);
    let create_account = system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        rent,
        Mint::LEN as u64,
        &TOKEN_2022_PROGRAM_ID,
    );
    let initialize_mint = token_2022_instruction::initialize_mint2(
        &TOKEN_2022_PROGRAM_ID,
        &mint.pubkey(),
        &authority,
        None,
        DECIMALS,
    )
    .unwrap();

    send_setup_transaction(
        svm,
        payer,
        vec![create_account, initialize_mint],
        &[payer, mint],
    );
}

fn create_token_account(
    svm: &mut LiteSVM,
    payer: &Keypair,
    token_account: &Keypair,
    mint: Pubkey,
    owner: Pubkey,
) {
    let rent = svm.minimum_balance_for_rent_exemption(TokenAccount::LEN);
    let create_account = system_instruction::create_account(
        &payer.pubkey(),
        &token_account.pubkey(),
        rent,
        TokenAccount::LEN as u64,
        &TOKEN_2022_PROGRAM_ID,
    );
    let initialize_account = token_2022_instruction::initialize_account3(
        &TOKEN_2022_PROGRAM_ID,
        &token_account.pubkey(),
        &mint,
        &owner,
    )
    .unwrap();

    send_setup_transaction(
        svm,
        payer,
        vec![create_account, initialize_account],
        &[payer, token_account],
    );
}

fn mint_to(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: Pubkey,
    destination: Pubkey,
    authority: &Keypair,
    amount: u64,
) {
    let ix = token_2022_instruction::mint_to_checked(
        &TOKEN_2022_PROGRAM_ID,
        &mint,
        &destination,
        &authority.pubkey(),
        &[],
        amount,
        DECIMALS,
    )
    .unwrap();

    send_setup_transaction(svm, payer, vec![ix], &[payer, authority]);
}

struct TokenFixture {
    trader: Keypair,
    base_mint: Keypair,
    base_source: Keypair,
    base_vault: Keypair,
    quote_vault: Keypair,
    base_destination: Keypair,
    quote_destination: Keypair,
    market: MarketAccounts,
    trader_balance: Pubkey,
}

fn initialized_market_with_token_accounts(svm: &mut LiteSVM, admin: Keypair) -> TokenFixture {
    let trader = Keypair::new();
    test_support::fund_user(svm, &trader);

    let base_mint = Keypair::new();
    let quote_mint = Keypair::new();
    let base_source = Keypair::new();
    let base_vault = Keypair::new();
    let quote_vault = Keypair::new();
    let base_destination = Keypair::new();
    let quote_destination = Keypair::new();

    create_mint(svm, &admin, &base_mint, admin.pubkey());
    create_mint(svm, &admin, &quote_mint, admin.pubkey());

    let protocol_config = protocol_config_pda();
    let market_config = market_config_pda(base_mint.pubkey(), quote_mint.pubkey());
    let vault_authority = vault_authority_pda(market_config);
    let trader_balance = trader_balance_pda(market_config, trader.pubkey());

    create_token_account(
        svm,
        &admin,
        &base_source,
        base_mint.pubkey(),
        trader.pubkey(),
    );
    create_token_account(
        svm,
        &admin,
        &base_vault,
        base_mint.pubkey(),
        vault_authority,
    );
    create_token_account(
        svm,
        &admin,
        &quote_vault,
        quote_mint.pubkey(),
        vault_authority,
    );
    create_token_account(
        svm,
        &admin,
        &base_destination,
        base_mint.pubkey(),
        trader.pubkey(),
    );
    create_token_account(
        svm,
        &admin,
        &quote_destination,
        quote_mint.pubkey(),
        trader.pubkey(),
    );
    mint_to(
        svm,
        &admin,
        base_mint.pubkey(),
        base_source.pubkey(),
        &admin,
        INITIAL_BASE_AMOUNT,
    );

    assert!(test_support::send_instruction(
        svm,
        &admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config),
    ));

    let market = MarketAccounts {
        protocol_config,
        market_config,
        base_mint: base_mint.pubkey(),
        quote_mint: quote_mint.pubkey(),
        base_vault: base_vault.pubkey(),
        quote_vault: quote_vault.pubkey(),
        vault_authority,
        settlement_authority: Keypair::new().pubkey(),
    };

    assert!(test_support::send_instruction(
        svm,
        &admin,
        initialize_market_ix(admin.pubkey(), &market),
    ));

    TokenFixture {
        trader,
        base_mint,
        base_source,
        base_vault,
        quote_vault,
        base_destination,
        quote_destination,
        market,
        trader_balance,
    }
}

fn deposit_base(svm: &mut LiteSVM, fixture: &TokenFixture, amount: u64) {
    assert!(test_support::send_instruction(
        svm,
        &fixture.trader,
        deposit_ix(DepositParams {
            trader: fixture.trader.pubkey(),
            market_config: fixture.market.market_config,
            trader_balance: fixture.trader_balance,
            source: fixture.base_source.pubkey(),
            vault: fixture.base_vault.pubkey(),
            mint: fixture.base_mint.pubkey(),
            asset: spot_settlement::CustodyAsset::Base,
            amount,
        }),
    ));
}

#[test]
fn base_deposit_moves_tokens_to_vault_and_credits_trader_balance() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market_with_token_accounts(&mut svm, admin);

    deposit_base(&mut svm, &fixture, DEPOSIT_AMOUNT);

    let balance = test_support::deserialize_account::<spot_settlement::TraderMarketBalance>(
        &svm,
        &fixture.trader_balance,
    );

    assert_eq!(balance.market_config, fixture.market.market_config);
    assert_eq!(balance.trader, fixture.trader.pubkey());
    assert_eq!(balance.available_base, DEPOSIT_AMOUNT);
    assert_eq!(balance.available_quote, 0);
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.base_source.pubkey()),
        INITIAL_BASE_AMOUNT - DEPOSIT_AMOUNT,
    );
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.base_vault.pubkey()),
        DEPOSIT_AMOUNT,
    );
}

#[test]
fn deposit_rejects_wrong_vault_for_selected_asset() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market_with_token_accounts(&mut svm, admin);

    test_support::assert_result_fails_with(
        test_support::send_instruction_result(
            &mut svm,
            &fixture.trader,
            deposit_ix(DepositParams {
                trader: fixture.trader.pubkey(),
                market_config: fixture.market.market_config,
                trader_balance: fixture.trader_balance,
                source: fixture.base_source.pubkey(),
                vault: fixture.quote_vault.pubkey(),
                mint: fixture.base_mint.pubkey(),
                asset: spot_settlement::CustodyAsset::Base,
                amount: DEPOSIT_AMOUNT,
            }),
        ),
        "Invalid market vault",
    );
}

#[test]
fn deposit_rejects_zero_amount() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market_with_token_accounts(&mut svm, admin);

    test_support::assert_result_fails_with(
        test_support::send_instruction_result(
            &mut svm,
            &fixture.trader,
            deposit_ix(DepositParams {
                trader: fixture.trader.pubkey(),
                market_config: fixture.market.market_config,
                trader_balance: fixture.trader_balance,
                source: fixture.base_source.pubkey(),
                vault: fixture.base_vault.pubkey(),
                mint: fixture.base_mint.pubkey(),
                asset: spot_settlement::CustodyAsset::Base,
                amount: 0,
            }),
        ),
        "Deposit amount must be greater than zero",
    );
}

#[test]
fn base_withdraw_moves_tokens_from_vault_and_debits_trader_balance() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market_with_token_accounts(&mut svm, admin);
    deposit_base(&mut svm, &fixture, DEPOSIT_AMOUNT);

    assert!(test_support::send_instruction(
        &mut svm,
        &fixture.trader,
        withdraw_ix(WithdrawParams {
            trader: fixture.trader.pubkey(),
            market_config: fixture.market.market_config,
            trader_balance: fixture.trader_balance,
            vault: fixture.base_vault.pubkey(),
            destination: fixture.base_destination.pubkey(),
            mint: fixture.base_mint.pubkey(),
            vault_authority: fixture.market.vault_authority,
            asset: spot_settlement::CustodyAsset::Base,
            amount: DEPOSIT_AMOUNT / 2,
        }),
    ));

    let balance = test_support::deserialize_account::<spot_settlement::TraderMarketBalance>(
        &svm,
        &fixture.trader_balance,
    );

    assert_eq!(balance.available_base, DEPOSIT_AMOUNT / 2);
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.base_vault.pubkey()),
        DEPOSIT_AMOUNT / 2,
    );
    assert_eq!(
        test_support::token_2022_account_amount(&svm, &fixture.base_destination.pubkey()),
        DEPOSIT_AMOUNT / 2,
    );
}

#[test]
fn withdraw_rejects_insufficient_available_balance() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market_with_token_accounts(&mut svm, admin);
    deposit_base(&mut svm, &fixture, DEPOSIT_AMOUNT);

    test_support::assert_result_fails_with(
        test_support::send_instruction_result(
            &mut svm,
            &fixture.trader,
            withdraw_ix(WithdrawParams {
                trader: fixture.trader.pubkey(),
                market_config: fixture.market.market_config,
                trader_balance: fixture.trader_balance,
                vault: fixture.base_vault.pubkey(),
                destination: fixture.base_destination.pubkey(),
                mint: fixture.base_mint.pubkey(),
                vault_authority: fixture.market.vault_authority,
                asset: spot_settlement::CustodyAsset::Base,
                amount: DEPOSIT_AMOUNT + 1,
            }),
        ),
        "Insufficient available balance",
    );
}

#[test]
fn withdraw_rejects_wrong_destination_for_selected_asset() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market_with_token_accounts(&mut svm, admin);
    deposit_base(&mut svm, &fixture, DEPOSIT_AMOUNT);

    test_support::assert_result_fails_with(
        test_support::send_instruction_result(
            &mut svm,
            &fixture.trader,
            withdraw_ix(WithdrawParams {
                trader: fixture.trader.pubkey(),
                market_config: fixture.market.market_config,
                trader_balance: fixture.trader_balance,
                vault: fixture.base_vault.pubkey(),
                destination: fixture.quote_destination.pubkey(),
                mint: fixture.base_mint.pubkey(),
                vault_authority: fixture.market.vault_authority,
                asset: spot_settlement::CustodyAsset::Base,
                amount: DEPOSIT_AMOUNT / 2,
            }),
        ),
        "Invalid withdraw destination account",
    );
}

#[test]
fn withdraw_rejects_zero_amount() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market_with_token_accounts(&mut svm, admin);
    deposit_base(&mut svm, &fixture, DEPOSIT_AMOUNT);

    test_support::assert_result_fails_with(
        test_support::send_instruction_result(
            &mut svm,
            &fixture.trader,
            withdraw_ix(WithdrawParams {
                trader: fixture.trader.pubkey(),
                market_config: fixture.market.market_config,
                trader_balance: fixture.trader_balance,
                vault: fixture.base_vault.pubkey(),
                destination: fixture.base_destination.pubkey(),
                mint: fixture.base_mint.pubkey(),
                vault_authority: fixture.market.vault_authority,
                asset: spot_settlement::CustodyAsset::Base,
                amount: 0,
            }),
        ),
        "Withdraw amount must be greater than zero",
    );
}
