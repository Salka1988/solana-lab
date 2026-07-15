use anchor_lang::prelude::*;

use crate::{
    constants::{MARKET_CONFIG_SEED, SETTLEMENT_RECEIPT_SEED, TRADER_MARKET_BALANCE_SEED},
    error::ErrorCode,
    state::{MarketConfig, SettlementReceipt, TraderMarketBalance},
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
    receipt.base_amount = base_amount;
    receipt.quote_amount = quote_amount;
    receipt.bump = ctx.bumps.settlement_receipt;

    Ok(())
}
