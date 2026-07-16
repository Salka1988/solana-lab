use anchor_lang::{prelude::*, solana_program::instruction::Instruction};
use sha2::{Digest, Sha256};
use solana_instructions_sysvar::get_instruction_relative;
use solana_sdk_ids::{ed25519_program, sysvar::instructions::ID as INSTRUCTIONS_SYSVAR_ID};

use crate::{
    constants::{
        MARKET_CONFIG_SEED, ORDER_FILL_STATE_SEED, SETTLEMENT_RECEIPT_SEED,
        TRADER_MARKET_BALANCE_SEED,
    },
    error::ErrorCode,
    state::{
        MarketConfig, OrderFillState, SettlementReceipt, SignedFillArgs, SignedOrderPayload,
        SignedOrderSide, TraderMarketBalance,
    },
};

#[derive(Accounts)]
#[instruction(settlement_id: u64)]
pub struct SettleFill<'info> {
    pub settlement_authority: Signer<'info>,

    #[account(
        seeds = [
            MARKET_CONFIG_SEED,
            market_config.base_mint.as_ref(),
            market_config.quote_mint.as_ref()
        ],
        bump = market_config.bump,
        has_one = settlement_authority @ ErrorCode::UnauthorizedSettlementAuthority
    )]
    pub market_config: Account<'info, MarketConfig>,

    /// CHECK: trader identity used for PDA derivation and receipt storage
    pub buyer: UncheckedAccount<'info>,

    /// CHECK: trader identity used for PDA derivation and receipt storage
    pub seller: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [
            TRADER_MARKET_BALANCE_SEED,
            market_config.key().as_ref(),
            buyer.key().as_ref()
        ],
        bump = buyer_balance.bump,
        constraint = buyer_balance.market_config == market_config.key(),
        constraint = buyer_balance.trader == buyer.key()
    )]
    pub buyer_balance: Account<'info, TraderMarketBalance>,

    #[account(
        mut,
        seeds = [
            TRADER_MARKET_BALANCE_SEED,
            market_config.key().as_ref(),
            seller.key().as_ref()
        ],
        bump = seller_balance.bump,
        constraint = seller_balance.market_config == market_config.key(),
        constraint = seller_balance.trader == seller.key()
    )]
    pub seller_balance: Account<'info, TraderMarketBalance>,

    #[account(
        init,
        payer = payer,
        space = SettlementReceipt::SPACE,
        seeds = [
            SETTLEMENT_RECEIPT_SEED,
            market_config.key().as_ref(),
            &settlement_id.to_le_bytes()
        ],
        bump
    )]
    pub settlement_receipt: Account<'info, SettlementReceipt>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn settle_fill_handler(
    ctx: Context<SettleFill>,
    settlement_id: u64,
    base_amount: u64,
    quote_amount: u64,
) -> Result<()> {
    require!(!ctx.accounts.market_config.paused, ErrorCode::MarketPaused);
    require!(base_amount > 0, ErrorCode::InvalidSettlementAmount);
    require!(quote_amount > 0, ErrorCode::InvalidSettlementAmount);

    let buyer_balance = &mut ctx.accounts.buyer_balance;
    buyer_balance.available_quote = buyer_balance
        .available_quote
        .checked_sub(quote_amount)
        .ok_or(ErrorCode::InsufficientAvailableBalance)?;
    buyer_balance.available_base = buyer_balance
        .available_base
        .checked_add(base_amount)
        .ok_or(ErrorCode::SettlementBalanceOverflow)?;

    let seller_balance = &mut ctx.accounts.seller_balance;
    seller_balance.available_base = seller_balance
        .available_base
        .checked_sub(base_amount)
        .ok_or(ErrorCode::InsufficientAvailableBalance)?;
    seller_balance.available_quote = seller_balance
        .available_quote
        .checked_add(quote_amount)
        .ok_or(ErrorCode::SettlementBalanceOverflow)?;

    let receipt = &mut ctx.accounts.settlement_receipt;
    receipt.market_config = ctx.accounts.market_config.key();
    receipt.settlement_id = settlement_id;
    receipt.buyer = ctx.accounts.buyer.key();
    receipt.seller = ctx.accounts.seller.key();
    receipt.buyer_order_hash = [0; 32];
    receipt.seller_order_hash = [0; 32];
    receipt.base_amount = base_amount;
    receipt.quote_amount = quote_amount;
    receipt.bump = ctx.bumps.settlement_receipt;

    Ok(())
}

#[derive(Accounts)]
#[instruction(settlement_id: u64, buyer_order_hash: [u8; 32], seller_order_hash: [u8; 32])]
pub struct SettleSignedFill<'info> {
    pub settlement_authority: Signer<'info>,

    #[account(
        seeds = [
            MARKET_CONFIG_SEED,
            market_config.base_mint.as_ref(),
            market_config.quote_mint.as_ref()
        ],
        bump = market_config.bump,
        has_one = settlement_authority @ ErrorCode::UnauthorizedSettlementAuthority
    )]
    pub market_config: Box<Account<'info, MarketConfig>>,

    /// CHECK: trader identity validated against signed order payload
    pub buyer: UncheckedAccount<'info>,

    /// CHECK: trader identity validated against signed order payload
    pub seller: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [
            TRADER_MARKET_BALANCE_SEED,
            market_config.key().as_ref(),
            buyer.key().as_ref()
        ],
        bump = buyer_balance.bump,
        constraint = buyer_balance.market_config == market_config.key(),
        constraint = buyer_balance.trader == buyer.key()
    )]
    pub buyer_balance: Box<Account<'info, TraderMarketBalance>>,

    #[account(
        mut,
        seeds = [
            TRADER_MARKET_BALANCE_SEED,
            market_config.key().as_ref(),
            seller.key().as_ref()
        ],
        bump = seller_balance.bump,
        constraint = seller_balance.market_config == market_config.key(),
        constraint = seller_balance.trader == seller.key()
    )]
    pub seller_balance: Box<Account<'info, TraderMarketBalance>>,

    #[account(
        init_if_needed,
        payer = payer,
        space = OrderFillState::SPACE,
        seeds = [
            ORDER_FILL_STATE_SEED,
            market_config.key().as_ref(),
            buyer_order_hash.as_ref()
        ],
        bump
    )]
    pub buyer_order_fill_state: Box<Account<'info, OrderFillState>>,

    #[account(
        init_if_needed,
        payer = payer,
        space = OrderFillState::SPACE,
        seeds = [
            ORDER_FILL_STATE_SEED,
            market_config.key().as_ref(),
            seller_order_hash.as_ref()
        ],
        bump
    )]
    pub seller_order_fill_state: Box<Account<'info, OrderFillState>>,

    #[account(
        init,
        payer = payer,
        space = SettlementReceipt::SPACE,
        seeds = [
            SETTLEMENT_RECEIPT_SEED,
            market_config.key().as_ref(),
            &settlement_id.to_le_bytes()
        ],
        bump
    )]
    pub settlement_receipt: Box<Account<'info, SettlementReceipt>>,

    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: checked by Solana instructions sysvar loader
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    pub instructions_sysvar: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn settle_signed_fill_handler(
    ctx: Context<SettleSignedFill>,
    settlement_id: u64,
    buyer_order_hash: [u8; 32],
    seller_order_hash: [u8; 32],
    args: SignedFillArgs,
) -> Result<()> {
    require!(!ctx.accounts.market_config.paused, ErrorCode::MarketPaused);
    require!(args.fill_price > 0, ErrorCode::InvalidSettlementAmount);
    require!(args.fill_quantity > 0, ErrorCode::InvalidSettlementAmount);
    require!(
        args.settlement_id == settlement_id
            && args.buyer_order_hash == buyer_order_hash
            && args.seller_order_hash == seller_order_hash,
        ErrorCode::InvalidSignedOrder
    );

    let market = ctx.accounts.market_config.key();
    validate_signed_order_payload(
        args.buyer_order,
        market,
        ctx.accounts.buyer.key(),
        SignedOrderSide::Bid,
    )?;
    validate_signed_order_payload(
        args.seller_order,
        market,
        ctx.accounts.seller.key(),
        SignedOrderSide::Ask,
    )?;
    validate_signed_fill_rules(args, Clock::get()?.slot)?;

    let buyer_preimage = args.buyer_order.signing_preimage();
    let seller_preimage = args.seller_order.signing_preimage();
    require!(
        sha256_32(&buyer_preimage) == args.buyer_order_hash,
        ErrorCode::OrderHashMismatch
    );
    require!(
        sha256_32(&seller_preimage) == args.seller_order_hash,
        ErrorCode::OrderHashMismatch
    );

    verify_previous_ed25519_instruction(
        ctx.accounts.instructions_sysvar.to_account_info(),
        -2,
        ctx.accounts.buyer.key(),
        args.buyer_signature,
        &buyer_preimage,
    )?;
    verify_previous_ed25519_instruction(
        ctx.accounts.instructions_sysvar.to_account_info(),
        -1,
        ctx.accounts.seller.key(),
        args.seller_signature,
        &seller_preimage,
    )?;

    initialize_fill_state_if_needed(
        &mut ctx.accounts.buyer_order_fill_state,
        market,
        args.buyer_order_hash,
        ctx.bumps.buyer_order_fill_state,
    )?;
    initialize_fill_state_if_needed(
        &mut ctx.accounts.seller_order_fill_state,
        market,
        args.seller_order_hash,
        ctx.bumps.seller_order_fill_state,
    )?;

    apply_fill_state(
        &mut ctx.accounts.buyer_order_fill_state,
        args.buyer_order.quantity,
        args.fill_quantity,
    )?;
    apply_fill_state(
        &mut ctx.accounts.seller_order_fill_state,
        args.seller_order.quantity,
        args.fill_quantity,
    )?;

    let quote_amount = quote_amount_u64(args.fill_price, args.fill_quantity)?;
    apply_balance_settlement(
        &mut ctx.accounts.buyer_balance,
        &mut ctx.accounts.seller_balance,
        args.fill_quantity,
        quote_amount,
    )?;

    let receipt = &mut ctx.accounts.settlement_receipt;
    receipt.market_config = market;
    receipt.settlement_id = args.settlement_id;
    receipt.buyer = ctx.accounts.buyer.key();
    receipt.seller = ctx.accounts.seller.key();
    receipt.buyer_order_hash = args.buyer_order_hash;
    receipt.seller_order_hash = args.seller_order_hash;
    receipt.base_amount = args.fill_quantity;
    receipt.quote_amount = quote_amount;
    receipt.bump = ctx.bumps.settlement_receipt;

    Ok(())
}

#[derive(Accounts)]
#[instruction(order_hash: [u8; 32])]
pub struct CancelSignedOrder<'info> {
    pub trader: Signer<'info>,

    #[account(
        seeds = [
            MARKET_CONFIG_SEED,
            market_config.base_mint.as_ref(),
            market_config.quote_mint.as_ref()
        ],
        bump = market_config.bump
    )]
    pub market_config: Box<Account<'info, MarketConfig>>,

    #[account(
        init_if_needed,
        payer = payer,
        space = OrderFillState::SPACE,
        seeds = [
            ORDER_FILL_STATE_SEED,
            market_config.key().as_ref(),
            order_hash.as_ref()
        ],
        bump
    )]
    pub order_fill_state: Box<Account<'info, OrderFillState>>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn cancel_signed_order_handler(
    ctx: Context<CancelSignedOrder>,
    order_hash: [u8; 32],
    order: SignedOrderPayload,
) -> Result<()> {
    require!(!ctx.accounts.market_config.paused, ErrorCode::MarketPaused);

    let market = ctx.accounts.market_config.key();
    validate_cancel_order_payload(order, market, ctx.accounts.trader.key())?;
    require!(
        sha256_32(&order.signing_preimage()) == order_hash,
        ErrorCode::OrderHashMismatch
    );

    initialize_fill_state_if_needed(
        &mut ctx.accounts.order_fill_state,
        market,
        order_hash,
        ctx.bumps.order_fill_state,
    )?;
    ctx.accounts.order_fill_state.cancelled = true;

    Ok(())
}

fn validate_signed_order_payload(
    order: SignedOrderPayload,
    market: Pubkey,
    trader: Pubkey,
    side: SignedOrderSide,
) -> Result<()> {
    require!(order.market_config == market, ErrorCode::InvalidSignedOrder);
    require!(order.trader == trader, ErrorCode::InvalidSignedOrder);
    require!(order.side == side, ErrorCode::InvalidSignedOrder);
    require!(order.order_id > 0, ErrorCode::InvalidSignedOrder);
    require!(order.price > 0, ErrorCode::InvalidSignedOrder);
    require!(order.quantity > 0, ErrorCode::InvalidSignedOrder);
    require!(order.nonce > 0, ErrorCode::InvalidSignedOrder);
    require!(order.expiry_slot > 0, ErrorCode::InvalidSignedOrder);
    Ok(())
}

fn validate_cancel_order_payload(
    order: SignedOrderPayload,
    market: Pubkey,
    trader: Pubkey,
) -> Result<()> {
    require!(order.market_config == market, ErrorCode::InvalidSignedOrder);
    require!(order.trader == trader, ErrorCode::InvalidSignedOrder);
    require!(order.order_id > 0, ErrorCode::InvalidSignedOrder);
    require!(order.price > 0, ErrorCode::InvalidSignedOrder);
    require!(order.quantity > 0, ErrorCode::InvalidSignedOrder);
    require!(order.nonce > 0, ErrorCode::InvalidSignedOrder);
    require!(order.expiry_slot > 0, ErrorCode::InvalidSignedOrder);
    Ok(())
}

fn validate_signed_fill_rules(args: SignedFillArgs, current_slot: u64) -> Result<()> {
    require!(
        args.buyer_order.trader != args.seller_order.trader,
        ErrorCode::InvalidSignedOrder
    );
    require!(
        current_slot <= args.buyer_order.expiry_slot
            && current_slot <= args.seller_order.expiry_slot,
        ErrorCode::SignedOrderExpired
    );
    require!(
        args.buyer_order.price >= args.seller_order.price,
        ErrorCode::SignedOrderPricesDoNotCross
    );
    require!(
        args.fill_price <= args.buyer_order.price && args.fill_price >= args.seller_order.price,
        ErrorCode::FillPriceOutsideSignedOrder
    );
    require!(
        args.fill_quantity <= args.buyer_order.quantity
            && args.fill_quantity <= args.seller_order.quantity,
        ErrorCode::FillQuantityExceedsSignedOrder
    );
    Ok(())
}

fn verify_previous_ed25519_instruction(
    instructions_sysvar: AccountInfo,
    relative_index: i64,
    expected_pubkey: Pubkey,
    expected_signature: [u8; 64],
    expected_message: &[u8],
) -> Result<()> {
    let instruction = get_instruction_relative(relative_index, &instructions_sysvar)?;
    verify_ed25519_instruction(
        &instruction,
        expected_pubkey.as_ref(),
        &expected_signature,
        expected_message,
    )
}

fn verify_ed25519_instruction(
    instruction: &Instruction,
    expected_pubkey: &[u8],
    expected_signature: &[u8],
    expected_message: &[u8],
) -> Result<()> {
    require!(
        instruction.program_id == ed25519_program::ID,
        ErrorCode::InvalidEd25519Instruction
    );
    require!(
        instruction.accounts.is_empty(),
        ErrorCode::InvalidEd25519Instruction
    );
    require!(
        instruction.data.len() >= 16 && instruction.data[0] == 1,
        ErrorCode::InvalidEd25519Instruction
    );

    let signature_offset = read_u16(&instruction.data, 2)?;
    let signature_instruction_index = read_u16(&instruction.data, 4)?;
    let pubkey_offset = read_u16(&instruction.data, 6)?;
    let pubkey_instruction_index = read_u16(&instruction.data, 8)?;
    let message_offset = read_u16(&instruction.data, 10)?;
    let message_size = read_u16(&instruction.data, 12)?;
    let message_instruction_index = read_u16(&instruction.data, 14)?;

    require!(
        signature_instruction_index == u16::MAX
            && pubkey_instruction_index == u16::MAX
            && message_instruction_index == u16::MAX,
        ErrorCode::InvalidEd25519Instruction
    );

    require_slice_eq(
        instruction.data.as_slice(),
        signature_offset,
        expected_signature,
    )?;
    require_slice_eq(instruction.data.as_slice(), pubkey_offset, expected_pubkey)?;
    require_slice_eq(
        instruction.data.as_slice(),
        message_offset,
        expected_message,
    )?;
    require!(
        usize::from(message_size) == expected_message.len(),
        ErrorCode::InvalidEd25519Instruction
    );

    Ok(())
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or(ErrorCode::InvalidEd25519Instruction)?;
    Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
}

fn require_slice_eq(data: &[u8], offset: u16, expected: &[u8]) -> Result<()> {
    let offset = usize::from(offset);
    let actual = data
        .get(offset..offset + expected.len())
        .ok_or(ErrorCode::InvalidEd25519Instruction)?;
    require!(actual == expected, ErrorCode::InvalidEd25519Instruction);
    Ok(())
}

fn initialize_fill_state_if_needed(
    fill_state: &mut Account<OrderFillState>,
    market_config: Pubkey,
    order_hash: [u8; 32],
    bump: u8,
) -> Result<()> {
    if fill_state.market_config == Pubkey::default() {
        fill_state.market_config = market_config;
        fill_state.order_hash = order_hash;
        fill_state.filled_quantity = 0;
        fill_state.cancelled = false;
        fill_state.bump = bump;
    }

    require!(
        fill_state.market_config == market_config && fill_state.order_hash == order_hash,
        ErrorCode::OrderFillStateMismatch
    );

    Ok(())
}

fn apply_fill_state(
    fill_state: &mut Account<OrderFillState>,
    order_quantity: u64,
    fill_quantity: u64,
) -> Result<()> {
    require!(!fill_state.cancelled, ErrorCode::OrderCancelled);
    let new_filled_quantity = fill_state
        .filled_quantity
        .checked_add(fill_quantity)
        .ok_or(ErrorCode::SettlementBalanceOverflow)?;
    require!(
        new_filled_quantity <= order_quantity,
        ErrorCode::FillQuantityExceedsSignedOrder
    );
    fill_state.filled_quantity = new_filled_quantity;
    Ok(())
}

fn quote_amount_u64(price: u64, quantity: u64) -> Result<u64> {
    let quote_amount = u128::from(price)
        .checked_mul(u128::from(quantity))
        .ok_or(ErrorCode::SettlementBalanceOverflow)?;
    u64::try_from(quote_amount).map_err(|_| ErrorCode::SettlementBalanceOverflow.into())
}

fn sha256_32(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn apply_balance_settlement(
    buyer_balance: &mut Account<TraderMarketBalance>,
    seller_balance: &mut Account<TraderMarketBalance>,
    base_amount: u64,
    quote_amount: u64,
) -> Result<()> {
    buyer_balance.available_quote = buyer_balance
        .available_quote
        .checked_sub(quote_amount)
        .ok_or(ErrorCode::InsufficientAvailableBalance)?;
    buyer_balance.available_base = buyer_balance
        .available_base
        .checked_add(base_amount)
        .ok_or(ErrorCode::SettlementBalanceOverflow)?;

    seller_balance.available_base = seller_balance
        .available_base
        .checked_sub(base_amount)
        .ok_or(ErrorCode::InsufficientAvailableBalance)?;
    seller_balance.available_quote = seller_balance
        .available_quote
        .checked_add(quote_amount)
        .ok_or(ErrorCode::SettlementBalanceOverflow)?;

    Ok(())
}
