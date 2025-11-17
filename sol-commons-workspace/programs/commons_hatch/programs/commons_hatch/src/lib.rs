use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use anchor_lang::solana_program::clock::Clock;
use anchor_lang::solana_program::hash;

declare_id!("CPjQgH9wbaJsW57qB1aaHasgv6MZAgQLwF1D77WZm2Uv");

#[program]
pub mod commons_hatch {
    use super::*;

    pub fn initialize_hatch(
        ctx: Context<InitializeHatch>,
        min_raise: u64,
        max_raise: u64,
        open_slot: u64,
        close_slot: u64,
        merkle_root: [u8; 32],
    ) -> Result<()> {
        let hatch_config = &mut ctx.accounts.hatch_config;
        hatch_config.reserve_asset_mint = ctx.accounts.reserve_asset_mint.key();
        hatch_config.min_raise = min_raise;
        hatch_config.max_raise = max_raise;
        hatch_config.open_slot = open_slot;
        hatch_config.close_slot = close_slot;
        hatch_config.merkle_root = merkle_root;
        hatch_config.total_raised = 0;
        hatch_config.finalized = false;
        hatch_config.failed = false;
        hatch_config.hatch_config_bump = ctx.bumps.hatch_config; // Store the bump
        hatch_config.hatch_vault = ctx.accounts.hatch_vault.key(); // Store hatch_vault key
        Ok(())
    }

    pub fn contribute(
        ctx: Context<Contribute>,
        amount: u64,
        proof: Vec<[u8; 32]>,
    ) -> Result<()> {
        // Verify Merkle proof
        let leaf = get_leaf_from_contributor(&ctx.accounts.authority.key(), amount);
        require!(verify_merkle_proof(proof, ctx.accounts.hatch_config.merkle_root, leaf), HatchError::InvalidMerkleProof);

        // Transfer reserve tokens from user to hatch vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_reserve_token_account.to_account_info(),
            to: ctx.accounts.hatch_vault.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // Update contribution account
        let contribution = &mut ctx.accounts.contribution;
        contribution.amount += amount;

        // Update total raised
        let hatch_config = &mut ctx.accounts.hatch_config;
        hatch_config.total_raised += amount;

        Ok(())
    }

    pub fn finalize_hatch(ctx: Context<FinalizeHatch>) -> Result<()> {
        let hatch_config = &mut ctx.accounts.hatch_config;
        let clock = Clock::get()?;

        // Check if the hatch has been finalized already
        if hatch_config.finalized {
            return err!(HatchError::AlreadyFinalized);
        }

        // Check if the hatch is still open
        if clock.slot < hatch_config.close_slot {
            return err!(HatchError::HatchStillOpen);
        }

        // Check if the minimum raise has been met
        if hatch_config.total_raised < hatch_config.min_raise {
            hatch_config.failed = true;
            return Ok(());
        }

        hatch_config.finalized = true;

        // TODO: CPI to commons_abc to initialize the curve
        // TODO: Mint commons tokens to contributors

        Ok(())
    }

    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        let hatch_config = &mut ctx.accounts.hatch_config;
        let contribution = &mut ctx.accounts.contribution;

        // Check if the hatch failed
        if !hatch_config.failed {
            return err!(HatchError::HatchNotFailed);
        }

        // Check if the contribution has already been refunded
        if contribution.refunded {
            return err!(HatchError::AlreadyRefunded);
        }

        // Transfer tokens from hatch vault to user
        let cpi_accounts = Transfer {
            from: ctx.accounts.hatch_vault.to_account_info(),
            to: ctx.accounts.user_reserve_token_account.to_account_info(),
            authority: ctx.accounts.hatch_config.to_account_info(), // PDA authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let seeds = &[
            b"hatch_config",
            ctx.accounts.hatch_config.reserve_asset_mint.as_ref(),
            &[ctx.accounts.hatch_config.hatch_config_bump],
        ];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, contribution.amount)?;

        contribution.refunded = true;

        Ok(())
    }

    pub fn claim(ctx: Context<Claim>) -> Result<()> {
        let hatch_config = &mut ctx.accounts.hatch_config;
        let contribution = &mut ctx.accounts.contribution;

        // Check if the hatch succeeded
        if !hatch_config.finalized || hatch_config.failed {
            return err!(HatchError::HatchNotSucceeded);
        }

        // Check if the contribution has already been claimed
        if contribution.claimed {
            return err!(HatchError::AlreadyClaimed);
        }

        // TODO: Calculate proportional share of commons tokens
        // TODO: Mint commons tokens to user

        contribution.claimed = true;

        Ok(())
    }

    pub fn close_hatch(ctx: Context<CloseHatch>) -> Result<()> {
        let hatch_config = &mut ctx.accounts.hatch_config;

        // Check if the hatch failed
        if !hatch_config.failed {
            return err!(HatchError::HatchNotFailed);
        }

        // TODO: Check if all contributions have been refunded

        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeHatch<'info> {
    #[account(init, payer = authority, space = 8 + 32 + 8 + 8 + 8 + 8 + 32 + 8 + 1 + 1 + 1 + 32, seeds = [b"hatch_config", reserve_asset_mint.key().as_ref()], bump)] // Added 32 bytes for hatch_vault
    pub hatch_config: Account<'info, HatchConfig>,
    pub reserve_asset_mint: Account<'info, Mint>,
    #[account(init, payer = authority, token::mint = reserve_asset_mint, token::authority = hatch_config)]
    pub hatch_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Contribute<'info> {
    #[account(mut)]
    pub hatch_config: Account<'info, HatchConfig>,
    #[account(
        init_if_needed,
        payer = authority,
        space = 8 + 8 + 1 + 1, // Added 1 byte for claimed
        seeds = [b"contribution", authority.key().as_ref()],
        bump
    )]
    pub contribution: Account<'info, Contribution>,
    #[account(
        mut,
        seeds = [b"hatch_vault", hatch_config.reserve_asset_mint.as_ref()], // Updated seeds
        bump = hatch_config.hatch_config_bump, // Use hatch_config_bump for hatch_vault
    )]
    pub hatch_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_reserve_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct FinalizeHatch<'info> {
    #[account(mut)]
    pub hatch_config: Account<'info, HatchConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct Refund<'info> {
    #[account(mut)]
    pub hatch_config: Account<'info, HatchConfig>,
    #[account(mut)]
    pub contribution: Account<'info, Contribution>,
    #[account(mut)]
    pub hatch_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_reserve_token_account: Account<'info, TokenAccount>,
    /// CHECK: This is the PDA authority for the hatch vault
    #[account(seeds = [b"hatch_config", hatch_config.reserve_asset_mint.as_ref()], bump = hatch_config.hatch_config_bump)]
    pub hatch_config_authority: Account<'info, HatchConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut)]
    pub hatch_config: Account<'info, HatchConfig>,
    #[account(mut)]
    pub contribution: Account<'info, Contribution>,
    #[account(mut)]
    pub user_commons_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CloseHatch<'info> {
    #[account(mut, close = authority)]
    pub hatch_config: Account<'info, HatchConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
}

#[account]
pub struct HatchConfig {
    pub reserve_asset_mint: Pubkey,
    pub min_raise: u64,
    pub max_raise: u64,
    pub open_slot: u64,
    pub close_slot: u64,
    pub merkle_root: [u8; 32],
    pub total_raised: u64,
    pub finalized: bool,
    pub failed: bool,
    pub hatch_config_bump: u8,
    pub hatch_vault: Pubkey, // Add this field
}

#[account]
pub struct Contribution {
    pub amount: u64,
    pub refunded: bool,
    pub claimed: bool, // Add this field
}

// Helper function to create a leaf from the contributor's public key and amount
fn get_leaf_from_contributor(contributor: &Pubkey, amount: u64) -> [u8; 32] {
    let mut data = contributor.to_bytes().to_vec();
    data.extend_from_slice(&amount.to_le_bytes());
    hash::hashv(&[&data]).to_bytes()
}

// Helper function to verify Merkle proof
fn verify_merkle_proof(
    proof: Vec<[u8; 32]>,
    root: [u8; 32],
    leaf: [u8; 32],
) -> bool {
    let mut computed_hash = leaf;
    for proof_element in proof {
        if computed_hash <= proof_element {
            // Hash(computed_hash + proof_element)
            computed_hash = hash::hashv(&[&computed_hash, &proof_element]).to_bytes();
        } else {
            // Hash(proof_element + computed_hash)
            computed_hash = hash::hashv(&[&proof_element, &computed_hash]).to_bytes();
        }
    }
    computed_hash == root
}

#[error_code]
pub enum HatchError {
    #[msg("Hatch is still open.")]
    HatchStillOpen,
    #[msg("Hatch has already been finalized.")]
    AlreadyFinalized,
    #[msg("Invalid Merkle proof.")]
    InvalidMerkleProof,
    #[msg("Hatch has not failed.")]
    HatchNotFailed,
    #[msg("Contribution has already been refunded.")]
    AlreadyRefunded,
    #[msg("Hatch has not succeeded.")]
    HatchNotSucceeded,
    #[msg("Contribution has already been claimed.")]
    AlreadyClaimed,
}