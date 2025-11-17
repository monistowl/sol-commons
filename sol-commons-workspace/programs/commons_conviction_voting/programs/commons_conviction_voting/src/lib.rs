use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::clock::Clock;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

const CV_SCALE: u128 = 1_000_000;
const CV_SCALE_U64: u64 = 1_000_000;

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
        require!(decay_rate <= CV_SCALE_U64, CustomError::InvalidDecayRate);
        require!(max_ratio <= CV_SCALE_U64, CustomError::InvalidMaxRatio);
        require!(
            min_threshold <= CV_SCALE_U64,
            CustomError::InvalidMinThreshold
        );
        cv_config.decay_rate = decay_rate;
        cv_config.max_ratio = max_ratio;
        cv_config.weight_exponent = weight_exponent;
        cv_config.min_threshold = min_threshold;
        cv_config.commons_treasury = ctx.accounts.commons_treasury.key();
        cv_config.commons_token_mint = ctx.accounts.commons_token_mint.key();
        cv_config.cv_config_bump = ctx.bumps.cv_config;
        cv_config.staking_vault = ctx.accounts.staking_vault.key();
        cv_config.staking_vault_bump = ctx.bumps.staking_vault;
        cv_config.authority = ctx.accounts.authority.key();
        cv_config.total_staked = 0;
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
        require!(amount > 0, CustomError::InvalidStakeAmount);
        let stake_account = &mut ctx.accounts.stake_account;
        let proposal = &mut ctx.accounts.proposal;
        let cv_config = &mut ctx.accounts.cv_config;
        let slot = ctx.accounts.clock.slot;
        require!(
            proposal.status == ProposalStatus::Pending,
            CustomError::ProposalNotPending
        );

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
        stake_account.staked_amount = stake_account
            .staked_amount
            .checked_add(amount)
            .ok_or(CustomError::StakeOverflow)?;
        stake_account.last_update_slot = slot;
        stake_account.authority = ctx.accounts.authority.key();

        cv_config.total_staked = cv_config
            .total_staked
            .checked_add(amount)
            .ok_or(CustomError::StakeOverflow)?;

        update_conviction_for_proposal(proposal, amount as i128, cv_config, slot)?;
        Ok(())
    }

    pub fn unstake_tokens(ctx: Context<UnstakeTokens>, amount: u64) -> Result<()> {
        require!(amount > 0, CustomError::InvalidStakeAmount);
        let stake_account = &mut ctx.accounts.stake_account;
        let proposal = &mut ctx.accounts.proposal;
        let slot = ctx.accounts.clock.slot;

        require!(
            proposal.status == ProposalStatus::Pending,
            CustomError::ProposalNotPending
        );
        require!(
            stake_account.staked_amount >= amount,
            CustomError::InsufficientStakedAmount
        );

        let bump = {
            let cv_config = &mut ctx.accounts.cv_config;
            update_conviction_for_proposal(proposal, -(amount as i128), cv_config, slot)?;

            cv_config.total_staked = cv_config
                .total_staked
                .checked_sub(amount)
                .ok_or(CustomError::StakeUnderflow)?;

            cv_config.cv_config_bump
        };

        stake_account.staked_amount = stake_account
            .staked_amount
            .checked_sub(amount)
            .ok_or(CustomError::InsufficientStakedAmount)?;
        stake_account.last_update_slot = slot;

        // Transfer Commons tokens from staking vault to user
        let cpi_accounts = Transfer {
            from: ctx.accounts.staking_vault.to_account_info(),
            to: ctx.accounts.user_commons_token_account.to_account_info(),
            authority: ctx.accounts.cv_config.to_account_info(), // PDA authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let seeds = &[b"cv_config".as_ref(), &[bump]];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, amount)?;

        Ok(())
    }

    pub fn check_and_execute(ctx: Context<CheckAndExecute>) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        require!(
            proposal.status == ProposalStatus::Pending,
            CustomError::ProposalNotPending
        );

        let (required, bump) = {
            let cv_config = &ctx.accounts.cv_config;
            (
                compute_required_conviction(
                    proposal.requested_amount,
                    ctx.accounts.commons_treasury.amount,
                    cv_config.total_staked,
                    cv_config,
                )?,
                cv_config.cv_config_bump,
            )
        };

        require!(
            proposal.current_conviction >= required,
            CustomError::ThresholdNotReached
        );

        proposal.status = ProposalStatus::Approved;
        proposal.last_update_slot = ctx.accounts.clock.slot;

        // Transfer requested_amount from commons_treasury to recipient
        let cpi_accounts = Transfer {
            from: ctx.accounts.commons_treasury.to_account_info(),
            to: ctx.accounts.recipient_token_account.to_account_info(),
            authority: ctx.accounts.cv_config.to_account_info(), // PDA authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let seeds = &[b"cv_config".as_ref(), &[bump]];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, proposal.requested_amount)?;

        Ok(())
    }

    pub fn withdraw_stake(ctx: Context<WithdrawStake>) -> Result<()> {
        let stake_account = &mut ctx.accounts.stake_account;
        let proposal = &mut ctx.accounts.proposal;
        let slot = ctx.accounts.clock.slot;

        require!(
            proposal.status != ProposalStatus::Pending,
            CustomError::ProposalStillPending
        );

        let amount = stake_account.staked_amount;
        if amount > 0 {
            let bump = {
                let cv_config = &mut ctx.accounts.cv_config;
                update_conviction_for_proposal(proposal, -(amount as i128), cv_config, slot)?;

                cv_config.total_staked = cv_config
                    .total_staked
                    .checked_sub(amount)
                    .ok_or(CustomError::StakeUnderflow)?;

                cv_config.cv_config_bump
            };

            // Transfer Commons tokens from staking vault to user
            let cpi_accounts = Transfer {
                from: ctx.accounts.staking_vault.to_account_info(),
                to: ctx.accounts.user_commons_token_account.to_account_info(),
                authority: ctx.accounts.cv_config.to_account_info(), // PDA authority
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let seeds = &[b"cv_config".as_ref(), &[bump]];
            let signer = &[&seeds[..]];
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
            token::transfer(cpi_ctx, amount)?;
        }

        stake_account.close(ctx.accounts.authority.to_account_info())?;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}

#[derive(Accounts)]
#[instruction(decay_rate: u64, max_ratio: u64, weight_exponent: u64, min_threshold: u64)]
pub struct InitializeCvConfig<'info> {
    #[account(init, payer = authority, space = 178, seeds = [b"cv_config"], bump)]
    pub cv_config: Account<'info, CVConfig>,
    pub commons_treasury: Account<'info, TokenAccount>,
    pub commons_token_mint: Account<'info, Mint>,
    #[account(init,
        payer = authority,
        token::mint = commons_token_mint,
        token::authority = cv_config,
        seeds = [b"staking_vault", cv_config.key().as_ref()],
        bump
    )]
    pub staking_vault: Account<'info, TokenAccount>,
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
    #[account(init_if_needed, payer = authority, space = 8 + 32 + 32 + 8 + 8 + 32, seeds = [b"stake", authority.key().as_ref(), proposal.key().as_ref()], bump)]
    // Added 32 bytes for authority
    pub stake_account: Account<'info, Stake>,
    #[account(mut)]
    pub cv_config: Account<'info, CVConfig>,
    #[account(mut)]
    pub proposal: Account<'info, Proposal>,
    pub commons_token_mint: Account<'info, Mint>,
    #[account(mut)]
    pub user_commons_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [b"staking_vault", cv_config.key().as_ref()],
        bump = cv_config.staking_vault_bump,
        token::authority = cv_config,
        token::mint = commons_token_mint
    )]
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
    pub commons_token_mint: Account<'info, Mint>,
    #[account(
        mut,
        seeds = [b"staking_vault", cv_config.key().as_ref()],
        bump = cv_config.staking_vault_bump,
        token::authority = cv_config,
        token::mint = commons_token_mint
    )]
    pub staking_vault: Account<'info, TokenAccount>, // Global staking vault
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
}

#[derive(Accounts)]
pub struct CheckAndExecute<'info> {
    #[account(mut, has_one = authority)]
    pub cv_config: Account<'info, CVConfig>,
    #[account(mut)]
    pub proposal: Account<'info, Proposal>,
    #[account(mut)]
    pub commons_treasury: Account<'info, TokenAccount>,
    #[account(mut)]
    pub recipient_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
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
    pub proposal: Account<'info, Proposal>,
    #[account(mut)]
    pub user_commons_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub commons_token_mint: Account<'info, Mint>,
    /// CHECK: This is the staking vault PDA
    #[account(
        mut,
        seeds = [b"staking_vault", cv_config.key().as_ref()],
        bump = cv_config.staking_vault_bump,
        token::authority = cv_config,
        token::mint = commons_token_mint
    )]
    pub staking_vault: Account<'info, TokenAccount>, // Global staking vault
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub clock: Sysvar<'info, Clock>,
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
    pub staking_vault: Pubkey,
    pub staking_vault_bump: u8,
    pub authority: Pubkey,
    pub total_staked: u64,
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

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub enum ProposalStatus {
    Pending,
    Approved,
    Rejected,
}

#[error_code]
pub enum CustomError {
    #[msg("Insufficient staked amount")]
    InsufficientStakedAmount,
    #[msg("Stake amount must be greater than zero")]
    InvalidStakeAmount,
    #[msg("Stake total overflow")]
    StakeOverflow,
    #[msg("Stake total underflow")]
    StakeUnderflow,
    #[msg("Proposal must remain pending to stake or unstake")]
    ProposalNotPending,
    #[msg("Proposal still pending")]
    ProposalStillPending,
    #[msg("Requested amount exceeds configured spending limit")]
    SpendingLimitExceeded,
    #[msg("Conviction threshold not reached")]
    ThresholdNotReached,
    #[msg("Decay rate must be <= 1")]
    InvalidDecayRate,
    #[msg("Max ratio must be <= 1")]
    InvalidMaxRatio,
    #[msg("Min threshold must be <= 1")]
    InvalidMinThreshold,
    #[msg("Treasury has no funds")]
    EmptyTreasury,
}

fn compute_required_conviction(
    requested: u64,
    treasury_balance: u64,
    total_staked: u64,
    config: &CVConfig,
) -> Result<u64> {
    require!(treasury_balance > 0, CustomError::EmptyTreasury);

    let max_allowed =
        (treasury_balance as u128).saturating_mul(config.max_ratio as u128) / CV_SCALE;
    let max_allowed_u64 = max_allowed.min(u64::MAX as u128) as u64;
    require!(max_allowed_u64 > 0, CustomError::SpendingLimitExceeded);
    require!(
        requested <= max_allowed_u64,
        CustomError::SpendingLimitExceeded
    );

    let effective_stake = total_staked.max(1);
    let min_conviction = (effective_stake as u128 * config.min_threshold as u128) / CV_SCALE;
    let request_ratio = (requested as u128 * CV_SCALE) / treasury_balance as u128;
    let weighted_ratio = request_ratio.saturating_mul(config.weight_exponent as u128) / CV_SCALE;
    let dynamic_conviction = weighted_ratio.saturating_mul(effective_stake as u128) / CV_SCALE;

    let required = min_conviction.max(dynamic_conviction);
    Ok(required.min(u64::MAX as u128) as u64)
}

fn update_conviction_for_proposal(
    proposal: &mut Proposal,
    delta: i128,
    config: &CVConfig,
    slot: u64,
) -> Result<()> {
    let elapsed = slot.saturating_sub(proposal.last_update_slot);
    let decayed = decay_conviction(proposal.current_conviction, config.decay_rate, elapsed);
    let updated = if delta >= 0 {
        decayed.saturating_add(delta as u128)
    } else {
        decayed.saturating_sub((-delta) as u128)
    };
    proposal.current_conviction = updated.min(u64::MAX as u128) as u64;
    proposal.last_update_slot = slot;
    Ok(())
}

fn decay_conviction(current: u64, decay_rate: u64, elapsed_slots: u64) -> u128 {
    if elapsed_slots == 0 || current == 0 {
        return current as u128;
    }
    if decay_rate == 0 {
        return 0;
    }
    let factor = scaled_pow(decay_rate as u128, elapsed_slots);
    current as u128 * factor / CV_SCALE
}

fn scaled_pow(mut base: u128, mut exp: u64) -> u128 {
    if exp == 0 {
        return CV_SCALE;
    }
    if base == 0 {
        return 0;
    }
    let mut result = CV_SCALE;
    while exp > 0 {
        if exp & 1 == 1 {
            result = result.saturating_mul(base).saturating_div(CV_SCALE);
        }
        exp >>= 1;
        if exp > 0 {
            base = base.saturating_mul(base).saturating_div(CV_SCALE);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::prelude::Pubkey;

    fn base_config() -> CVConfig {
        CVConfig {
            decay_rate: CV_SCALE_U64 / 2,
            max_ratio: CV_SCALE_U64,
            weight_exponent: CV_SCALE_U64,
            min_threshold: CV_SCALE_U64 / 10,
            commons_treasury: Pubkey::default(),
            commons_token_mint: Pubkey::default(),
            cv_config_bump: 0,
            staking_vault: Pubkey::default(),
            staking_vault_bump: 0,
            authority: Pubkey::default(),
            total_staked: 0,
        }
    }

    fn base_proposal() -> Proposal {
        Proposal {
            creator: Pubkey::default(),
            requested_amount: 0,
            metadata_hash: String::new(),
            status: ProposalStatus::Pending,
            current_conviction: 0,
            last_update_slot: 0,
        }
    }

    #[test]
    fn scaled_pow_identity() {
        assert_eq!(scaled_pow(CV_SCALE, 0), CV_SCALE);
        assert_eq!(scaled_pow(CV_SCALE, 5), CV_SCALE);
    }

    #[test]
    fn scaled_pow_decay() {
        let half = CV_SCALE / 2;
        let squared = scaled_pow(half, 2);
        assert!(squared < half);
    }

    #[test]
    fn compute_required_conviction_uses_min_threshold() {
        let config = base_config();
        let required = compute_required_conviction(10, 1_000_000, 1_000, &config).unwrap();
        let expected = (1_000u128 * config.min_threshold as u128 / CV_SCALE) as u64;
        assert_eq!(required, expected);
    }

    #[test]
    fn compute_required_conviction_dynamic_overrides_minimum() {
        let config = base_config();
        let required = compute_required_conviction(800_000, 1_000_000, 1_000, &config).unwrap();
        assert_eq!(required, 800);
    }

    #[test]
    fn update_conviction_applies_decay_and_delta() {
        let mut proposal = base_proposal();
        proposal.current_conviction = 100;
        let config = base_config();

        update_conviction_for_proposal(&mut proposal, 200, &config, 1).unwrap();
        assert!(proposal.current_conviction > 100);
        let peak = proposal.current_conviction;

        update_conviction_for_proposal(&mut proposal, -20, &config, 2).unwrap();
        assert!(proposal.current_conviction < peak);
    }
}
