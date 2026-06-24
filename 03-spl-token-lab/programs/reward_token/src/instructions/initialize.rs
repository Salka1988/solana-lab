use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Initialize {}

pub fn initialize_handler(_ctx: Context<Initialize>) -> Result<()> {
    msg!("Initialized reward token scaffold");
    Ok(())
}
