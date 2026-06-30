use crate::{
    constants::{ISSUER_SEED, PROTOCOL_SEED},
    state::{IssuerConfig, ProtocolConfig},
};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetIssuerPaused<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        has_one = admin
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [ISSUER_SEED, issuer_authority.key().as_ref()],
        bump = issuer_config.bump,
        constraint = issuer_config.protocol_config == protocol_config.key(),
        constraint = issuer_config.authority == issuer_authority.key()
    )]
    pub issuer_config: Account<'info, IssuerConfig>,

    /// CHECK: Identity key used to derive and validate the issuer config PDA.
    pub issuer_authority: UncheckedAccount<'info>,
}

pub fn set_issuer_paused_handler(ctx: Context<SetIssuerPaused>, paused: bool) -> Result<()> {
    let issuer_config = &mut ctx.accounts.issuer_config;
    issuer_config.paused = paused;

    msg!(
        "Set issuer {} paused status to {}",
        issuer_config.authority,
        paused
    );

    Ok(())
}
