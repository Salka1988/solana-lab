use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountSerialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_account::Account,
    solana_ed25519_program::new_ed25519_instruction_with_signature,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const BUYER_QUOTE_DEPOSIT: u64 = 10_000;
const SELLER_BASE_DEPOSIT: u64 = 500;
const BASE_AMOUNT: u64 = 125;
const QUOTE_AMOUNT: u64 = 2_500;
const SETTLEMENT_ID: u64 = 42;
const SIGNED_FILL_PRICE: u64 = 20;
const SIGNED_FILL_QUANTITY: u64 = 100;

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

fn order_fill_state_pda(market_config: Pubkey, order_hash: [u8; 32]) -> Pubkey {
    Pubkey::find_program_address(
        &[
            spot_settlement::ORDER_FILL_STATE_SEED,
            market_config.as_ref(),
            order_hash.as_ref(),
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

struct SettleSignedFillParams<'a> {
    authority: Pubkey,
    market: &'a MarketAccounts,
    buyer: Pubkey,
    seller: Pubkey,
    buyer_balance: Pubkey,
    seller_balance: Pubkey,
    buyer_order_fill_state: Pubkey,
    seller_order_fill_state: Pubkey,
    settlement_id: u64,
    payer: Pubkey,
    args: spot_settlement::SignedFillArgs,
}

fn settle_signed_fill_ix(params: SettleSignedFillParams<'_>) -> Instruction {
    Instruction::new_with_bytes(
        spot_settlement::id(),
        &spot_settlement::instruction::SettleSignedFill {
            settlement_id: params.settlement_id,
            buyer_order_hash: params.args.buyer_order_hash,
            seller_order_hash: params.args.seller_order_hash,
            args: params.args,
        }
        .data(),
        spot_settlement::accounts::SettleSignedFill {
            settlement_authority: params.authority,
            market_config: params.market.market_config,
            buyer: params.buyer,
            seller: params.seller,
            buyer_balance: params.buyer_balance,
            seller_balance: params.seller_balance,
            buyer_order_fill_state: params.buyer_order_fill_state,
            seller_order_fill_state: params.seller_order_fill_state,
            settlement_receipt: settlement_receipt_pda(
                params.market.market_config,
                params.settlement_id,
            ),
            payer: params.payer,
            instructions_sysvar: solana_sdk_ids::sysvar::instructions::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

struct CancelSignedOrderParams<'a> {
    trader: Pubkey,
    market: &'a MarketAccounts,
    order_hash: [u8; 32],
    order: spot_settlement::SignedOrderPayload,
    payer: Pubkey,
}

fn cancel_signed_order_ix(params: CancelSignedOrderParams<'_>) -> Instruction {
    Instruction::new_with_bytes(
        spot_settlement::id(),
        &spot_settlement::instruction::CancelSignedOrder {
            order_hash: params.order_hash,
            order: params.order,
        }
        .data(),
        spot_settlement::accounts::CancelSignedOrder {
            trader: params.trader,
            market_config: params.market.market_config,
            order_fill_state: order_fill_state_pda(params.market.market_config, params.order_hash),
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

fn signed_order(
    market_config: Pubkey,
    trader: Pubkey,
    side: spot_settlement::SignedOrderSide,
    order_id: u64,
    price: u64,
    quantity: u64,
) -> spot_settlement::SignedOrderPayload {
    spot_settlement::SignedOrderPayload {
        order_id,
        market_config,
        trader,
        side,
        price,
        quantity,
        nonce: order_id,
        expiry_slot: u64::MAX,
    }
}

fn order_hash(order: spot_settlement::SignedOrderPayload) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    Sha256::digest(order.signing_preimage()).into()
}

fn signed_fill_args(
    buyer: &Keypair,
    seller: &Keypair,
    market_config: Pubkey,
    settlement_id: u64,
    fill_quantity: u64,
) -> spot_settlement::SignedFillArgs {
    let buyer_order = signed_order(
        market_config,
        buyer.pubkey(),
        spot_settlement::SignedOrderSide::Bid,
        10,
        21,
        SIGNED_FILL_QUANTITY,
    );
    let seller_order = signed_order(
        market_config,
        seller.pubkey(),
        spot_settlement::SignedOrderSide::Ask,
        20,
        19,
        SIGNED_FILL_QUANTITY,
    );
    let buyer_preimage = buyer_order.signing_preimage();
    let seller_preimage = seller_order.signing_preimage();

    spot_settlement::SignedFillArgs {
        settlement_id,
        fill_price: SIGNED_FILL_PRICE,
        fill_quantity,
        buyer_order_hash: order_hash(buyer_order),
        seller_order_hash: order_hash(seller_order),
        buyer_order,
        buyer_signature: *buyer.sign_message(&buyer_preimage).as_array(),
        seller_order,
        seller_signature: *seller.sign_message(&seller_preimage).as_array(),
    }
}

fn resign_signed_fill_args(
    buyer: &Keypair,
    seller: &Keypair,
    mut args: spot_settlement::SignedFillArgs,
) -> spot_settlement::SignedFillArgs {
    let buyer_preimage = args.buyer_order.signing_preimage();
    let seller_preimage = args.seller_order.signing_preimage();

    args.buyer_order_hash = order_hash(args.buyer_order);
    args.seller_order_hash = order_hash(args.seller_order);
    args.buyer_signature = *buyer.sign_message(&buyer_preimage).as_array();
    args.seller_signature = *seller.sign_message(&seller_preimage).as_array();
    args
}

fn signed_fill_instructions(
    fixture: &Fixture,
    args: spot_settlement::SignedFillArgs,
) -> Vec<Instruction> {
    let buyer_preimage = args.buyer_order.signing_preimage();
    let seller_preimage = args.seller_order.signing_preimage();

    vec![
        new_ed25519_instruction_with_signature(
            &buyer_preimage,
            &args.buyer_signature,
            fixture.buyer.pubkey().as_array(),
        ),
        new_ed25519_instruction_with_signature(
            &seller_preimage,
            &args.seller_signature,
            fixture.seller.pubkey().as_array(),
        ),
        settle_signed_fill_ix(SettleSignedFillParams {
            authority: fixture.settlement_authority.pubkey(),
            market: &fixture.market,
            buyer: fixture.buyer.pubkey(),
            seller: fixture.seller.pubkey(),
            buyer_balance: fixture.buyer_balance,
            seller_balance: fixture.seller_balance,
            buyer_order_fill_state: order_fill_state_pda(
                fixture.market.market_config,
                args.buyer_order_hash,
            ),
            seller_order_fill_state: order_fill_state_pda(
                fixture.market.market_config,
                args.seller_order_hash,
            ),
            settlement_id: args.settlement_id,
            payer: fixture.settlement_authority.pubkey(),
            args,
        }),
    ]
}

fn send_transaction_with_metadata(
    svm: &mut LiteSVM,
    fee_payer: Pubkey,
    instructions: Vec<Instruction>,
    signers: &[&Keypair],
) -> litesvm::types::TransactionMetadata {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&instructions, Some(&fee_payer), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();

    svm.send_transaction(tx).unwrap()
}

fn measure_trusted_settlement_cu() -> u64 {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let meta = send_transaction_with_metadata(
        &mut svm,
        fixture.settlement_authority.pubkey(),
        vec![settle_fill_ix(SettleFillParams {
            authority: fixture.settlement_authority.pubkey(),
            market: &fixture.market,
            buyer: fixture.buyer.pubkey(),
            seller: fixture.seller.pubkey(),
            buyer_balance: fixture.buyer_balance,
            seller_balance: fixture.seller_balance,
            settlement_id: 200,
            payer: fixture.settlement_authority.pubkey(),
        })],
        &[&fixture.settlement_authority],
    );

    meta.compute_units_consumed
}

fn measure_signed_settlement_cu() -> u64 {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        201,
        SIGNED_FILL_QUANTITY,
    );
    let meta = send_transaction_with_metadata(
        &mut svm,
        fixture.settlement_authority.pubkey(),
        signed_fill_instructions(&fixture, args),
        &[&fixture.settlement_authority],
    );

    meta.compute_units_consumed
}

fn measure_cancel_signed_order_cu() -> u64 {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    let args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        202,
        SIGNED_FILL_QUANTITY,
    );
    let meta = send_transaction_with_metadata(
        &mut svm,
        fixture.buyer.pubkey(),
        vec![cancel_signed_order_ix(CancelSignedOrderParams {
            trader: fixture.buyer.pubkey(),
            market: &fixture.market,
            order_hash: args.buyer_order_hash,
            order: args.buyer_order,
            payer: fixture.buyer.pubkey(),
        })],
        &[&fixture.buyer],
    );

    meta.compute_units_consumed
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

#[test]
fn signed_settlement_requires_ed25519_precompiles_and_updates_fill_state() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        100,
        SIGNED_FILL_QUANTITY,
    );

    assert!(test_support::send_transaction(
        &mut svm,
        fixture.settlement_authority.pubkey(),
        signed_fill_instructions(&fixture, args),
        &[&fixture.settlement_authority],
    )
    .is_ok());

    let buyer_balance = test_support::deserialize_account::<spot_settlement::TraderMarketBalance>(
        &svm,
        &fixture.buyer_balance,
    );
    let seller_balance = test_support::deserialize_account::<spot_settlement::TraderMarketBalance>(
        &svm,
        &fixture.seller_balance,
    );
    let buyer_fill_state = test_support::deserialize_account::<spot_settlement::OrderFillState>(
        &svm,
        &order_fill_state_pda(fixture.market.market_config, args.buyer_order_hash),
    );
    let receipt = test_support::deserialize_account::<spot_settlement::SettlementReceipt>(
        &svm,
        &settlement_receipt_pda(fixture.market.market_config, args.settlement_id),
    );

    assert_eq!(buyer_balance.available_base, SIGNED_FILL_QUANTITY);
    assert_eq!(
        buyer_balance.available_quote,
        BUYER_QUOTE_DEPOSIT - SIGNED_FILL_PRICE * SIGNED_FILL_QUANTITY
    );
    assert_eq!(
        seller_balance.available_base,
        SELLER_BASE_DEPOSIT - SIGNED_FILL_QUANTITY
    );
    assert_eq!(
        seller_balance.available_quote,
        SIGNED_FILL_PRICE * SIGNED_FILL_QUANTITY
    );
    assert_eq!(buyer_fill_state.filled_quantity, SIGNED_FILL_QUANTITY);
    assert_eq!(receipt.buyer_order_hash, args.buyer_order_hash);
    assert_eq!(receipt.seller_order_hash, args.seller_order_hash);
}

#[test]
fn signed_settlement_rejects_mismatched_ed25519_message() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        101,
        SIGNED_FILL_QUANTITY,
    );
    let mut instructions = signed_fill_instructions(&fixture, args);
    let wrong_message = b"different message";
    let wrong_signature = *fixture.buyer.sign_message(wrong_message).as_array();
    instructions[0] = new_ed25519_instruction_with_signature(
        wrong_message,
        &wrong_signature,
        fixture.buyer.pubkey().as_array(),
    );

    test_support::assert_result_fails_with(
        test_support::send_transaction(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            instructions,
            &[&fixture.settlement_authority],
        ),
        "InvalidEd25519Instruction",
    );
}

#[test]
fn signed_settlement_tracks_partial_fills_and_rejects_overfill() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);

    let first = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        102,
        60,
    );
    assert!(test_support::send_transaction(
        &mut svm,
        fixture.settlement_authority.pubkey(),
        signed_fill_instructions(&fixture, first),
        &[&fixture.settlement_authority],
    )
    .is_ok());

    let second = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        103,
        41,
    );
    test_support::assert_result_fails_with(
        test_support::send_transaction(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            signed_fill_instructions(&fixture, second),
            &[&fixture.settlement_authority],
        ),
        "Fill quantity exceeds signed order remaining quantity",
    );
}

#[test]
fn cancelled_signed_order_rejects_future_settlement() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        104,
        SIGNED_FILL_QUANTITY,
    );

    assert!(test_support::send_instruction_with_signers(
        &mut svm,
        fixture.buyer.pubkey(),
        cancel_signed_order_ix(CancelSignedOrderParams {
            trader: fixture.buyer.pubkey(),
            market: &fixture.market,
            order_hash: args.buyer_order_hash,
            order: args.buyer_order,
            payer: fixture.buyer.pubkey(),
        }),
        &[&fixture.buyer],
    ));

    test_support::assert_result_fails_with(
        test_support::send_transaction(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            signed_fill_instructions(&fixture, args),
            &[&fixture.settlement_authority],
        ),
        "Order is cancelled",
    );
}

#[test]
fn partial_fill_then_cancel_rejects_remaining_fill() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let first = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        105,
        60,
    );

    assert!(test_support::send_transaction(
        &mut svm,
        fixture.settlement_authority.pubkey(),
        signed_fill_instructions(&fixture, first),
        &[&fixture.settlement_authority],
    )
    .is_ok());
    assert!(test_support::send_instruction_with_signers(
        &mut svm,
        fixture.buyer.pubkey(),
        cancel_signed_order_ix(CancelSignedOrderParams {
            trader: fixture.buyer.pubkey(),
            market: &fixture.market,
            order_hash: first.buyer_order_hash,
            order: first.buyer_order,
            payer: fixture.buyer.pubkey(),
        }),
        &[&fixture.buyer],
    ));

    let second = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        106,
        40,
    );
    test_support::assert_result_fails_with(
        test_support::send_transaction(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            signed_fill_instructions(&fixture, second),
            &[&fixture.settlement_authority],
        ),
        "Order is cancelled",
    );

    let buyer_fill_state = test_support::deserialize_account::<spot_settlement::OrderFillState>(
        &svm,
        &order_fill_state_pda(fixture.market.market_config, first.buyer_order_hash),
    );
    assert_eq!(buyer_fill_state.filled_quantity, 60);
    assert!(buyer_fill_state.cancelled);
}

#[test]
fn signed_order_cancel_rejects_wrong_trader() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    let attacker = Keypair::new();
    test_support::fund_user(&mut svm, &attacker);
    let args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        107,
        SIGNED_FILL_QUANTITY,
    );

    test_support::assert_result_fails_with(
        test_support::send_instruction_with_signers_result(
            &mut svm,
            attacker.pubkey(),
            cancel_signed_order_ix(CancelSignedOrderParams {
                trader: attacker.pubkey(),
                market: &fixture.market,
                order_hash: args.buyer_order_hash,
                order: args.buyer_order,
                payer: attacker.pubkey(),
            }),
            &[&attacker],
        ),
        "Invalid signed order",
    );
}

#[test]
fn settlement_compute_units_are_measured() {
    let trusted_settlement = measure_trusted_settlement_cu();
    let signed_settlement = measure_signed_settlement_cu();
    let cancel_signed_order = measure_cancel_signed_order_cu();

    println!("| Path | Compute units |");
    println!("| --- | ---: |");
    println!("| trusted settlement | {trusted_settlement} |");
    println!("| signed settlement | {signed_settlement} |");
    println!("| cancel signed order | {cancel_signed_order} |");

    assert!(trusted_settlement > 0);
    assert!(signed_settlement > trusted_settlement);
    assert!(cancel_signed_order > 0);
}

#[test]
fn signed_settlement_rejects_expired_order() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let mut args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        108,
        SIGNED_FILL_QUANTITY,
    );
    args.buyer_order.expiry_slot = 0;
    args = resign_signed_fill_args(&fixture.buyer, &fixture.seller, args);

    test_support::assert_result_fails_with(
        test_support::send_transaction(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            signed_fill_instructions(&fixture, args),
            &[&fixture.settlement_authority],
        ),
        "Invalid signed order",
    );

    let mut args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        109,
        SIGNED_FILL_QUANTITY,
    );
    args.buyer_order.expiry_slot = 1;
    args = resign_signed_fill_args(&fixture.buyer, &fixture.seller, args);
    svm.warp_to_slot(2);

    test_support::assert_result_fails_with(
        test_support::send_transaction(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            signed_fill_instructions(&fixture, args),
            &[&fixture.settlement_authority],
        ),
        "Signed order is expired",
    );
}

#[test]
fn signed_settlement_rejects_wrong_market_payload() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let mut args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        110,
        SIGNED_FILL_QUANTITY,
    );
    args.buyer_order.market_config = Keypair::new().pubkey();
    args = resign_signed_fill_args(&fixture.buyer, &fixture.seller, args);

    test_support::assert_result_fails_with(
        test_support::send_transaction(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            signed_fill_instructions(&fixture, args),
            &[&fixture.settlement_authority],
        ),
        "Invalid signed order",
    );
}

#[test]
fn signed_settlement_rejects_wrong_trader_payload() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let mut args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        111,
        SIGNED_FILL_QUANTITY,
    );
    args.buyer_order.trader = fixture.seller.pubkey();
    args = resign_signed_fill_args(&fixture.buyer, &fixture.seller, args);

    test_support::assert_result_fails_with(
        test_support::send_transaction(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            signed_fill_instructions(&fixture, args),
            &[&fixture.settlement_authority],
        ),
        "Invalid signed order",
    );
}

#[test]
fn signed_settlement_rejects_order_hash_mismatch() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let mut args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        112,
        SIGNED_FILL_QUANTITY,
    );
    args.buyer_order_hash = [9; 32];

    test_support::assert_result_fails_with(
        test_support::send_transaction(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            signed_fill_instructions(&fixture, args),
            &[&fixture.settlement_authority],
        ),
        "Order hash does not match signed order",
    );
}

#[test]
fn signed_settlement_rejects_non_crossing_prices() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let mut args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        113,
        SIGNED_FILL_QUANTITY,
    );
    args.buyer_order.price = 18;
    args.seller_order.price = 19;
    args.fill_price = 18;
    args = resign_signed_fill_args(&fixture.buyer, &fixture.seller, args);

    test_support::assert_result_fails_with(
        test_support::send_transaction(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            signed_fill_instructions(&fixture, args),
            &[&fixture.settlement_authority],
        ),
        "Signed order prices do not cross",
    );
}

#[test]
fn signed_settlement_rejects_fill_price_outside_signed_limits() {
    let (mut svm, admin) = setup();
    let fixture = initialized_market(&mut svm, admin);
    seed_trade_balances(&mut svm, &fixture);
    let mut args = signed_fill_args(
        &fixture.buyer,
        &fixture.seller,
        fixture.market.market_config,
        114,
        SIGNED_FILL_QUANTITY,
    );
    args.fill_price = args.buyer_order.price + 1;

    test_support::assert_result_fails_with(
        test_support::send_transaction(
            &mut svm,
            fixture.settlement_authority.pubkey(),
            signed_fill_instructions(&fixture, args),
            &[&fixture.settlement_authority],
        ),
        "Fill price is outside signed order limits",
    );
}
