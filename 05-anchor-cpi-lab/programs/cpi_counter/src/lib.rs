use anchor_lang::prelude::*;

declare_id!("HAajVKBhiN8g9UKSro4RdX9cnCV7ZsHs3esHetLAzrAJ");

pub const COUNTER_SEED: &[u8] = b"counter";

#[program]
pub mod cpi_counter {
    use super::*;

    pub fn initialize(ctx: Context<InitializeCounter>) -> Result<()> {
        msg!("Initialized CPI counter scaffold");
        let _ = ctx;
        Ok(())
    }

    pub fn initialize_counter(ctx: Context<InitializeCounterAccount>) -> Result<()> {
        let counter = &mut ctx.accounts.counter;

        counter.authority = ctx.accounts.authority.key();
        counter.count = 0;
        counter.bump = ctx.bumps.counter;

        Ok(())
    }

    pub fn increment(ctx: Context<Increment>) -> Result<()> {
        let counter = &mut ctx.accounts.counter;

        counter.count = counter
            .count
            .checked_add(1)
            .ok_or(ErrorCode::CounterOverflow)?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeCounter<'info> {
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitializeCounterAccount<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = payer,
        space = Counter::SPACE,
        seeds = [COUNTER_SEED, authority.key().as_ref()],
        bump
    )]
    pub counter: Account<'info, Counter>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Increment<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [COUNTER_SEED, authority.key().as_ref()],
        bump = counter.bump,
        has_one = authority
    )]
    pub counter: Account<'info, Counter>,
}

#[account]
pub struct Counter {
    pub authority: Pubkey,
    pub count: u64,
    pub bump: u8,
}

impl Counter {
    pub const SPACE: usize = 8 + 32 + 8 + 1;
}

#[error_code]
pub enum ErrorCode {
    #[msg("Counter overflow")]
    CounterOverflow,
}
