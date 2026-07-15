use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountSerialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_account::Account,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const BUYER_QUOTE_DEPOSIT: u64 = 10_000;
const SELLER_BASE_DEPOSIT: u64 = 500;
const BASE_AMOUNT: u64 = 125;
const QUOTE_AMOUNT: u64 = 2_500;
const SETTLEMENT_ID: u64 = 42;

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

fn settlement_receipt_pda(market_config: Pubkey, settlement_id: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            spot_settlement::SETTLEMENT_RECEIPT_SEED,
            market_config.as_ref(),
            &settlement_id.to_le_bytes(),
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

struct SettleFillParams<'a> {
    authority: Pubkey,
    market: &'a MarketAccounts,
    buyer: Pubkey,
    seller: Pubkey,
    buyer_balance: Pubkey,
    seller_balance: Pubkey,
    settlement_id: u64,
    payer: Pubkey,
}

fn settle_fill_ix(params: SettleFillParams<'_>) -> Instruction {
    Instruction::new_with_bytes(
        spot_settlement::id(),
        &spot_settlement::instruction::SettleFill {
            settlement_id: params.settlement_id,
            base_amount: BASE_AMOUNT,
            quote_amount: QUOTE_AMOUNT,
        }
        .data(),
        spot_settlement::accounts::SettleFill {
            settlement_authority: params.authority,
            market_config: params.market.market_config,
            buyer: params.buyer,
            seller: params.seller,
            buyer_balance: params.buyer_balance,
            seller_balance: params.seller_balance,
            settlement_receipt: settlement_receipt_pda(
                params.market.market_config,
                params.settlement_id,
            ),
            payer: params.payer,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

struct Fixture {
    admin: Keypair,
    settlement_authority: Keypair,
    buyer: Keypair,
    seller: Keypair,
    market: MarketAccounts,
    buyer_balance: Pubkey,
    seller_balance: Pubkey,
}

fn initialized_market(svm: &mut LiteSVM, admin: Keypair) -> Fixture {
    let settlement_authority = Keypair::new();
    let buyer = Keypair::new();
    let seller = Keypair::new();
    test_support::fund_user(svm, &settlement_authority);
    test_support::fund_user(svm, &buyer);
    test_support::fund_user(svm, &seller);

    let base_mint = Keypair::new().pubkey();
    let quote_mint = Keypair::new().pubkey();
    let protocol_config = protocol_config_pda();
    let market_config = market_config_pda(base_mint, quote_mint);
    let vault_authority = vault_authority_pda(market_config);

    assert!(test_support::send_instruction(
        svm,
        &admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config),
    ));

    let market = MarketAccounts {
        protocol_config,
        market_config,
        base_mint,
        quote_mint,
        base_vault: Keypair::new().pubkey(),
        quote_vault: Keypair::new().pubkey(),
        vault_authority,
        settlement_authority: settlement_authority.pubkey(),
    };

    assert!(test_support::send_instruction(
        svm,
        &admin,
        initialize_market_ix(admin.pubkey(), &market),
    ));

    Fixture {
        admin,
        settlement_authority,
        buyer_balance: trader_balance_pda(market_config, buyer.pubkey()),
        seller_balance: trader_balance_pda(market_config, seller.pubkey()),
        buyer,
        seller,
        market,
    }
}

fn seed_balance(
    svm: &mut LiteSVM,
    address: Pubkey,
    market_config: Pubkey,
    trader: Pubkey,
    available_base: u64,
    available_quote: u64,
) {
    let (_, bump) = Pubkey::find_program_address(
        &[
            spot_settlement::TRADER_MARKET_BALANCE_SEED,
            market_config.as_ref(),
            trader.as_ref(),
        ],
        &spot_settlement::id(),
    );
    let account = spot_settlement::TraderMarketBalance {
        market_config,
        trader,
        available_base,
        available_quote,
        bump,
    };
    let mut data = Vec::new();
    account.try_serialize(&mut data).unwrap();
    let lamports = svm.minimum_balance_for_rent_exemption(data.len());

    svm.set_account(
        address,
        Account {
            lamports,
            data,
            owner: spot_settlement::id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
}

fn seed_trade_balances(svm: &mut LiteSVM, fixture: &Fixture) {
    seed_balance(
        svm,
        fixture.buyer_balance,
        fixture.market.market_config,
        fixture.buyer.pubkey(),
        0,
        BUYER_QUOTE_DEPOSIT,
    );
    seed_balance(
        svm,
        fixture.seller_balance,
        fixture.market.market_config,
        fixture.seller.pubkey(),
        SELLER_BASE_DEPOSIT,
        0,
    );
}

#[test]
fn trusted_settlement_moves_internal_balances_and_records_receipt() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);

    assert!(test_support::send_instruction_with_signers(
        &mut svm,
        fixture.settlement_authority.pubkey(),
        settle_fill_ix(SettleFillParams {
            authority: fixture.settlement_authority.pubkey(),
            market: &fixture.market,
            buyer: fixture.buyer.pubkey(),
            seller: fixture.seller.pubkey(),
            buyer_balance: fixture.buyer_balance,
            seller_balance: fixture.seller_balance,
            settlement_id: SETTLEMENT_ID,
            payer: fixture.settlement_authority.pubkey(),
        }),
        &[&fixture.settlement_authority],
    ));

    let buyer_balance = test_support::deserialize_account::<spot_settlement::TraderMarketBalance>(
        &svm,
        &fixture.buyer_balance,
    );
    let seller_balance = test_support::deserialize_account::<spot_settlement::TraderMarketBalance>(
        &svm,
        &fixture.seller_balance,
    );
    let receipt = test_support::deserialize_account::<spot_settlement::SettlementReceipt>(
        &svm,
        &settlement_receipt_pda(fixture.market.market_config, SETTLEMENT_ID),
    );

    assert_eq!(buyer_balance.available_base, BASE_AMOUNT);
    assert_eq!(
        buyer_balance.available_quote,
        BUYER_QUOTE_DEPOSIT - QUOTE_AMOUNT
    );
    assert_eq!(
        seller_balance.available_base,
        SELLER_BASE_DEPOSIT - BASE_AMOUNT
    );
    assert_eq!(seller_balance.available_quote, QUOTE_AMOUNT);
    assert_eq!(receipt.market_config, fixture.market.market_config);
    assert_eq!(receipt.settlement_id, SETTLEMENT_ID);
    assert_eq!(receipt.buyer, fixture.buyer.pubkey());
    assert_eq!(receipt.seller, fixture.seller.pubkey());
    assert_eq!(receipt.base_amount, BASE_AMOUNT);
    assert_eq!(receipt.quote_amount, QUOTE_AMOUNT);
}

#[test]
fn settlement_rejects_replay_by_existing_receipt() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);

    let ix = |payer| {
        settle_fill_ix(SettleFillParams {
            authority: fixture.settlement_authority.pubkey(),
            market: &fixture.market,
            buyer: fixture.buyer.pubkey(),
            seller: fixture.seller.pubkey(),
            buyer_balance: fixture.buyer_balance,
            seller_balance: fixture.seller_balance,
            settlement_id: SETTLEMENT_ID,
            payer,
        })
    };

    assert!(test_support::send_instruction_with_signers(
        &mut svm,
        fixture.settlement_authority.pubkey(),
        ix(fixture.settlement_authority.pubkey()),
        &[&fixture.settlement_authority],
    ));
    test_support::assert_result_fails_with(
        test_support::send_instruction_with_signers_result(
            &mut svm,
            fixture.admin.pubkey(),
            ix(fixture.admin.pubkey()),
            &[&fixture.admin, &fixture.settlement_authority],
        ),
        "already in use",
    );
}

#[test]
fn settlement_rejects_unauthorized_authority() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let attacker = Keypair::new();
    test_support::fund_user(&mut svm, &attacker);

    test_support::assert_result_fails_with(
        test_support::send_instruction_with_signers_result(
            &mut svm,
            attacker.pubkey(),
            settle_fill_ix(SettleFillParams {
                authority: attacker.pubkey(),
                market: &fixture.market,
                buyer: fixture.buyer.pubkey(),
                seller: fixture.seller.pubkey(),
                buyer_balance: fixture.buyer_balance,
                seller_balance: fixture.seller_balance,
                settlement_id: SETTLEMENT_ID,
                payer: attacker.pubkey(),
            }),
            &[&attacker],
        ),
        "Only settlement authority can perform this action",
    );
}

#[test]
fn settlement_rejects_insufficient_balances() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_balance(
        &mut svm,
        fixture.buyer_balance,
        fixture.market.market_config,
        fixture.buyer.pubkey(),
        0,
        QUOTE_AMOUNT - 1,
    );
    seed_balance(
        &mut svm,
        fixture.seller_balance,
        fixture.market.market_config,
        fixture.seller.pubkey(),
        SELLER_BASE_DEPOSIT,
        0,
    );

    test_support::assert_result_fails_with(
        test_support::send_instruction_with_signers_result(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            settle_fill_ix(SettleFillParams {
                authority: fixture.settlement_authority.pubkey(),
                market: &fixture.market,
                buyer: fixture.buyer.pubkey(),
                seller: fixture.seller.pubkey(),
                buyer_balance: fixture.buyer_balance,
                seller_balance: fixture.seller_balance,
                settlement_id: SETTLEMENT_ID,
                payer: fixture.settlement_authority.pubkey(),
            }),
            &[&fixture.settlement_authority],
        ),
        "Insufficient available balance",
    );
}
