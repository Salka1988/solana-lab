use anchor_lang::prelude::*;

use crate::{constants::*, error::ErrorCode, state::*};

#[derive(Accounts)]
#[instruction(message: CrossChainMintMessage)]
pub struct ConsumeCrossChainMintMessage<'info> {
    pub bridge_authority: Signer<'info>,

    #[account(
        seeds = [BRIDGE_CONFIG_SEED, bridge_config.registered_mint.as_ref()],
        bump = bridge_config.bump,
        has_one = bridge_authority @ ErrorCode::UnauthorizedBridgeAuthority
    )]
    pub bridge_config: Account<'info, BridgeConfig>,

    #[account(
        init,
        payer = payer,
        space = ConsumedMessage::SPACE,
        seeds = [
            CONSUMED_MESSAGE_SEED,
            bridge_config.key().as_ref(),
            &message.source_chain_id.to_le_bytes(),
            &message.nonce.to_le_bytes()
        ],
        bump
    )]
    pub consumed_message: Account<'info, ConsumedMessage>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn consume_cross_chain_mint_message_handler(
    ctx: Context<ConsumeCrossChainMintMessage>,
    message: CrossChainMintMessage,
) -> Result<()> {
    require!(
        message.destination_chain_id == SOLANA_CHAIN_ID,
        ErrorCode::InvalidDestinationChain
    );
    require!(message.amount > 0, ErrorCode::InvalidMessageAmount);
    require!(
        message.recipient != Pubkey::default(),
        ErrorCode::InvalidRecipient
    );
    require!(
        message.mint == ctx.accounts.bridge_config.registered_mint,
        ErrorCode::UnregisteredMint
    );
    require!(
        message.amount <= ctx.accounts.bridge_config.per_message_limit,
        ErrorCode::BridgeLimitExceeded
    );

    let consumed_message = &mut ctx.accounts.consumed_message;
    consumed_message.bridge_config = ctx.accounts.bridge_config.key();
    consumed_message.source_chain_id = message.source_chain_id;
    consumed_message.nonce = message.nonce;
    consumed_message.mint = message.mint;
    consumed_message.recipient = message.recipient;
    consumed_message.amount = message.amount;
    consumed_message.bump = ctx.bumps.consumed_message;

    Ok(())
}
