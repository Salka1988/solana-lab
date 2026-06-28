use anchor_lang::prelude::*;
use cpi_counter::{self, Counter};

declare_id!("6qbLifa2aLfcbmqwypkS7Pcek4NXnMhCGe85gCz3qG6J");

pub const PROXY_AUTHORITY_SEED: &[u8] = b"proxy-authority";

#[program]
pub mod cpi_proxy {
    use super::*;

    pub fn initialize(ctx: Context<InitializeProxy>) -> Result<()> {
        msg!("Initialized CPI proxy scaffold");
        let _ = ctx;
        Ok(())
    }

    pub fn proxy_increment(ctx: Context<ProxyIncrement>) -> Result<()> {
        let cpi_accounts = cpi_counter::cpi::accounts::Increment {
            authority: ctx.accounts.authority.to_account_info(),
            counter: ctx.accounts.counter.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.cpi_counter_program.key(), cpi_accounts);

        cpi_counter::cpi::increment(cpi_ctx)
    }

    pub fn proxy_initialize_counter(ctx: Context<ProxyInitializeCounter>) -> Result<()> {
        let user = ctx.accounts.user.key();
        let signer_seeds: &[&[&[u8]]] = &[&[
            PROXY_AUTHORITY_SEED,
            user.as_ref(),
            &[ctx.bumps.proxy_authority],
        ]];
        let cpi_accounts = cpi_counter::cpi::accounts::InitializeCounterAccount {
            payer: ctx.accounts.user.to_account_info(),
            authority: ctx.accounts.proxy_authority.to_account_info(),
            counter: ctx.accounts.counter.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.cpi_counter_program.key(),
            cpi_accounts,
            signer_seeds,
        );

        cpi_counter::cpi::initialize_counter(cpi_ctx)
    }

    pub fn proxy_increment_with_signer(ctx: Context<ProxyIncrementWithSigner>) -> Result<()> {
        let user = ctx.accounts.user.key();
        let signer_seeds: &[&[&[u8]]] = &[&[
            PROXY_AUTHORITY_SEED,
            user.as_ref(),
            &[ctx.bumps.proxy_authority],
        ]];
        let cpi_accounts = cpi_counter::cpi::accounts::Increment {
            authority: ctx.accounts.proxy_authority.to_account_info(),
            counter: ctx.accounts.counter.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.cpi_counter_program.key(),
            cpi_accounts,
            signer_seeds,
        );

        cpi_counter::cpi::increment(cpi_ctx)
    }
}

#[derive(Accounts)]
pub struct InitializeProxy<'info> {
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ProxyIncrement<'info> {
    pub authority: Signer<'info>,
    #[account(mut)]
    pub counter: Account<'info, Counter>,
    pub cpi_counter_program: Program<'info, cpi_counter::program::CpiCounter>,
}

#[derive(Accounts)]
pub struct ProxyInitializeCounter<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    /// CHECK: PDA authority only; signs CPI with program seeds.
    #[account(seeds = [PROXY_AUTHORITY_SEED, user.key().as_ref()], bump)]
    pub proxy_authority: UncheckedAccount<'info>,
    /// CHECK: Created by cpi_counter during CPI.
    #[account(mut)]
    pub counter: UncheckedAccount<'info>,
    pub cpi_counter_program: Program<'info, cpi_counter::program::CpiCounter>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ProxyIncrementWithSigner<'info> {
    pub user: Signer<'info>,
    /// CHECK: PDA authority only; signs CPI with program seeds.
    #[account(seeds = [PROXY_AUTHORITY_SEED, user.key().as_ref()], bump)]
    pub proxy_authority: UncheckedAccount<'info>,
    #[account(mut)]
    pub counter: Account<'info, Counter>,
    pub cpi_counter_program: Program<'info, cpi_counter::program::CpiCounter>,
}
