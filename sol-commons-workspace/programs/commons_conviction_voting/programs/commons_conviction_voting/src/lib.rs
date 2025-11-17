use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use anchor_lang::solana_program::sysvar::clock::Clock;

declare_id!("sn9bNZ3gZxyiy5zE5FGGSJGQEXeedgoSGEMRQNUiSME");

#[program]
pub mod commons_conviction_voting {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }

    pub fn initialize_cv_config(
        ctx: Context<InitializeCvConfig>,
        decay_rate: u64,
        max_ratio: u64,
        weight_exponent: u64,
        min_threshold: u64,
    ) -> Result<()> {
        let cv_config = &mut ctx.accounts.cv_config;
        cv_config.decay_rate = decay_rate;
        cv_config.max_ratio = max_ratio;
        cv_config.weight_exponent = weight_exponent;
        cv_config.min_threshold = min_threshold;
        cv_config.commons_treasury = ctx.accounts.commons_treasury.key();
        cv_config.commons_token_mint = ctx.accounts.commons_token_mint.key();
        cv_config.cv_config_bump = ctx.bumps.cv_config;
        Ok(())
    }

    pub fn create_proposal(
        ctx: Context<CreateProposal>,
        requested_amount: u64,
        metadata_hash: String,
    ) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        proposal.creator = ctx.accounts.authority.key();
        proposal.requested_amount = requested_amount;
        proposal.metadata_hash = metadata_hash;
        proposal.status = ProposalStatus::Pending;
        proposal.current_conviction = 0;
        proposal.last_update_slot = Clock::get()?.slot;
        Ok(())
    }

    pub fn stake_tokens(ctx: Context<StakeTokens>, amount: u64) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let proposal = &mut ctx.accounts.proposal;
        let _cv_config = &ctx.accounts.cv_config; // Use _cv_config to avoid unused variable warning

        // Transfer Commons tokens from user to staking vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_commons_token_account.to_account_info(),
            to: ctx.accounts.staking_vault.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // Update stake account
        stake_account.user = ctx.accounts.authority.key();
        stake_account.proposal = proposal.key();
        stake_account.staked_amount += amount;
        stake_account.last_update_slot = Clock::get()?.slot;
        stake_account.authority = ctx.accounts.authority.key(); // Store the authority

        // TODO: Recompute user conviction and proposal conviction using exponential decay over elapsed time.
        // For now, just update proposal's current conviction
        proposal.current_conviction += amount; // Placeholder
        proposal.last_update_slot = Clock::get()?.slot;

        Ok(())
    }

    pub fn unstake_tokens(ctx: Context<UnstakeTokens>, amount: u64) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let proposal = &mut ctx.accounts.proposal;
        let cv_config = &ctx.accounts.cv_config;

        require!(stake_account.staked_amount >= amount, CustomError::InsufficientStakedAmount);

        // TODO: Recompute user conviction and proposal conviction using exponential decay over elapsed time.
        // For now, just update proposal's current conviction
        proposal.current_conviction -= amount; // Placeholder
        proposal.last_update_slot = Clock::get()?.slot;

        // Update stake account
        stake_account.staked_amount -= amount;
        stake_account.last_update_slot = Clock::get()?.slot;

        // Transfer Commons tokens from staking vault to user
        let cpi_accounts = Transfer {
            from: ctx.accounts.staking_vault.to_account_info(),
            to: ctx.accounts.user_commons_token_account.to_account_info(),
            authority: ctx.accounts.staking_vault_authority.to_account_info(), // PDA authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let seeds = &[
            b"staking_vault",
            cv_config.to_account_info().key.as_ref(),
            &[cv_config.cv_config_bump], // Assuming cv_config_bump is used for staking_vault PDA
        ];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, amount)?;

        Ok(())
    }

    pub fn check_and_execute(ctx: Context<CheckAndExecute>) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        let cv_config = &ctx.accounts.cv_config;

        // TODO: Recompute conviction since last update.
        // For now, assume current_conviction is up-to-date.

        // TODO: Compute threshold for requested funds based on CV function & available treasury.
        let threshold_met = true; // Placeholder

        if threshold_met {
            proposal.status = ProposalStatus::Approved;

            // Transfer requested_amount from commons_treasury to recipient
            let cpi_accounts = Transfer {
                from: ctx.accounts.commons_treasury.to_account_info(),
                to: ctx.accounts.recipient_token_account.to_account_info(),
                authority: ctx.accounts.cv_config_authority.to_account_info(), // PDA authority
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let seeds = &[
                b"cv_config",
                cv_config.commons_token_mint.as_ref(),
                &[cv_config.cv_config_bump],
            ];
            let signer = &[&seeds[..]];
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
            token::transfer(cpi_ctx, proposal.requested_amount)?;
        } else {
            // Otherwise just store updated conviction.
            // (current_conviction is already updated in stake/unstake, so nothing to do here for now)
        }

        Ok(())
    }

    pub fn withdraw_stake(ctx: Context<WithdrawStake>) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let cv_config = &ctx.accounts.cv_config;

        // TODO: Add checks to ensure proposal is rejected or executed before allowing withdrawal.

        // Transfer Commons tokens from staking vault to user
        let cpi_accounts = Transfer {
            from: ctx.accounts.staking_vault.to_account_info(),
            to: ctx.accounts.user_commons_token_account.to_account_info(),
            authority: ctx.accounts.staking_vault_authority.to_account_info(), // PDA authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let seeds = &[
            b"staking_vault",
            cv_config.to_account_info().key.as_ref(),
            &[cv_config.cv_config_bump], // Assuming cv_config_bump is used for staking_vault PDA
        ];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, stake_account.staked_amount)?;

        // Close stake account
        stake_account.close(ctx.accounts.authority.to_account_info())?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}

#[derive(Accounts)]
#[instruction(decay_rate: u64, max_ratio: u64, weight_exponent: u64, min_threshold: u64)]
pub struct InitializeCvConfig<'info> {
    #[account(init, payer = authority, space = 8 + 8 + 8 + 8 + 8 + 32 + 32 + 1, seeds = [b"cv_config"], bump)]
    pub cv_config: Account<'info, CVConfig>,
    pub commons_treasury: Account<'info, TokenAccount>,
    pub commons_token_mint: Account<'info, Mint>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(requested_amount: u64, metadata_hash: String)]
pub struct CreateProposal<'info> {
    #[account(init, payer = authority, space = 8 + 32 + 8 + 4 + 32 + 8 + 8, seeds = [b"proposal", authority.key().as_ref(), &requested_amount.to_le_bytes()], bump)]
    pub proposal: Account<'info, Proposal>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct StakeTokens<'info> {
    #[account(init_if_needed, payer = authority, space = 8 + 32 + 32 + 8 + 8 + 32, seeds = [b"stake", authority.key().as_ref(), proposal.key().as_ref()], bump)] // Added 32 bytes for authority
    pub stake_account: Account<'info, Stake>,
    #[account(mut)]
    pub cv_config: Account<'info, CVConfig>,
    #[account(mut)]
    pub proposal: Account<'info, Proposal>,
    #[account(mut)]
    pub user_commons_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    /// CHECK: This is the staking vault PDA
    pub staking_vault: Account<'info, TokenAccount>, // Global staking vault
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct UnstakeTokens<'info> {
    #[account(mut)]
    pub cv_config: Account<'info, CVConfig>,
    #[account(mut)]
    pub proposal: Account<'info, Proposal>,
    #[account(mut, has_one = authority)]
    pub stake_account: Account<'info, Stake>,
    #[account(mut)]
    pub user_commons_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    /// CHECK: This is the staking vault PDA
    pub staking_vault: Account<'info, TokenAccount>, // Global staking vault
    /// CHECK: This is the PDA authority for the staking vault
    pub staking_vault_authority: AccountInfo<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct CheckAndExecute<'info> {
    #[account(mut)]
    pub cv_config: Account<'info, CVConfig>,
    #[account(mut)]
    pub proposal: Account<'info, Proposal>,
    #[account(mut)]
    pub commons_treasury: Account<'info, TokenAccount>,
    #[account(mut)]
    pub recipient_token_account: Account<'info, TokenAccount>,
    /// CHECK: This is the PDA authority for the cv_config
    pub cv_config_authority: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct WithdrawStake<'info> {
    #[account(mut, has_one = authority, close = authority)]
    pub stake_account: Account<'info, Stake>,
    #[account(mut)]
    pub cv_config: Account<'info, CVConfig>,
    #[account(mut)]
    pub user_commons_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    /// CHECK: This is the staking vault PDA
    pub staking_vault: Account<'info, TokenAccount>, // Global staking vault
    /// CHECK: This is the PDA authority for the staking vault
    pub staking_vault_authority: AccountInfo<'info>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[account]
pub struct CVConfig {
    pub decay_rate: u64,
    pub max_ratio: u64,
    pub weight_exponent: u64,
    pub min_threshold: u64,
    pub commons_treasury: Pubkey,
    pub commons_token_mint: Pubkey,
    pub cv_config_bump: u8,
}

#[account]
pub struct Proposal {
    pub creator: Pubkey,
    pub requested_amount: u64,
    pub metadata_hash: String, // Max 32 bytes for hash
    pub status: ProposalStatus,
    pub current_conviction: u64,
    pub last_update_slot: u64,
}

#[account]
pub struct Stake {
    pub user: Pubkey,
    pub proposal: Pubkey,
    pub staked_amount: u64,
    pub last_update_slot: u64,
    pub authority: Pubkey, // Add this field
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum ProposalStatus {
    Pending,
    Approved,
    Rejected,
}

#[error_code]
pub enum CustomError {
    #[msg("Insufficient staked amount")]
    InsufficientStakedAmount,
}
