use anchor_lang::prelude::*;

declare_id!("iXB6Aag51itM754YzxveHUtMPwJyVDxBXkeXQgFPbsc");

#[program]
pub mod commons_tokenlog {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
