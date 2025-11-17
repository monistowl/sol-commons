use anchor_lang::prelude::*;

declare_id!("GUis4rZk6zLTMSMRiy68tN8sbwRMz27VpPfBDx34BHzo");

#[program]
pub mod sol_commons_workspace {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
