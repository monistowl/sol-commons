use anchor_lang::prelude::*;
use anchor_lang::solana_program::clock::Clock;
use anchor_lang::solana_program::hash;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount, Transfer};
use commons_abc::{
    self, cpi::accounts::InitializeCurve, program::CommonsAbc, CurveConfig, ID as COMMONS_ABC_ID,
};

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
        let (_vault_key, vault_bump) = Pubkey::find_program_address(
            &[
                b"hatch_vault",
                ctx.accounts.reserve_asset_mint.key().as_ref(),
            ],
            ctx.program_id,
        );
        hatch_config.hatch_vault_bump = vault_bump;
        hatch_config.commons_token_mint = Pubkey::default();
        hatch_config.commons_token_mint_bump = 0;
        hatch_config.curve_config = Pubkey::default();
        hatch_config.curve_config_bump = 0;
        hatch_config.reserve_vault = Pubkey::default();
        hatch_config.commons_treasury = Pubkey::default();
        hatch_config.total_refunded = 0;
        Ok(())
    }

    pub fn contribute(
        ctx: Context<Contribute>,
        amount: u64,
        allowed_allocation: u64,
        proof: Vec<[u8; 32]>,
    ) -> Result<()> {
        // Verify Merkle proof
        require!(allowed_allocation > 0, HatchError::InvalidAllocation);
        require!(amount > 0, HatchError::InvalidContributionAmount);
        let leaf = get_leaf_from_contributor_and_allocation(
            &ctx.accounts.authority.key(),
            allowed_allocation,
        );
        require!(
            verify_merkle_proof(proof, ctx.accounts.hatch_config.merkle_root, leaf),
            HatchError::InvalidMerkleProof
        );

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
        if contribution.max_allocation == 0 {
            contribution.max_allocation = allowed_allocation;
        } else {
            require!(
                contribution.max_allocation == allowed_allocation,
                HatchError::AllowedAllocationMismatch
            );
        }
        let new_total = contribution
            .amount
            .checked_add(amount)
            .ok_or(HatchError::AllocationExceeded)?;
        require!(
            new_total <= contribution.max_allocation,
            HatchError::AllocationExceeded
        );
        contribution.amount = new_total;

        // Update total raised
        let hatch_config = &mut ctx.accounts.hatch_config;
        hatch_config.total_raised += amount;

        Ok(())
    }

    pub fn finalize_hatch(
        ctx: Context<FinalizeHatch>,
        kappa: u64,
        exponent: u64,
        initial_price: u64,
        friction: u64,
    ) -> Result<()> {
        let hatch_config = &mut ctx.accounts.hatch_config;
        let clock = Clock::get()?;

        require!(!hatch_config.finalized, HatchError::AlreadyFinalized);
        require!(
            clock.slot >= hatch_config.close_slot,
            HatchError::HatchStillOpen
        );

        if hatch_config.total_raised < hatch_config.min_raise {
            hatch_config.failed = true;
            return Ok(());
        }

        require!(
            hatch_config.total_raised <= hatch_config.max_raise,
            HatchError::RaiseTooHigh
        );

        require!(
            ctx.accounts.reserve_asset_mint.key() == hatch_config.reserve_asset_mint,
            HatchError::IncorrectReserveMint
        );

        let (curve_config_key, curve_config_bump) = Pubkey::find_program_address(
            &[
                b"curve_config",
                ctx.accounts.commons_token_mint.key().as_ref(),
            ],
            &COMMONS_ABC_ID,
        );
        require!(
            curve_config_key == ctx.accounts.curve_config.key(),
            HatchError::InvalidCurveConfig
        );

        let cpi_accounts = InitializeCurve {
            curve_config: ctx.accounts.curve_config.to_account_info(),
            commons_token_mint: ctx.accounts.commons_token_mint.to_account_info(),
            reserve_mint: ctx.accounts.reserve_asset_mint.to_account_info(),
            reserve_vault: ctx.accounts.reserve_vault.to_account_info(),
            commons_treasury: ctx.accounts.commons_treasury.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            token_program: ctx.accounts.token_program.to_account_info(),
            rent: ctx.accounts.rent.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(
            ctx.accounts.commons_abc_program.to_account_info(),
            cpi_accounts,
        );
        commons_abc::cpi::initialize_curve(
            cpi_ctx,
            kappa,
            exponent,
            initial_price,
            friction,
            hatch_config.total_raised,
            hatch_config.total_raised,
        )?;

        let (_commons_token_mint_key, commons_token_mint_bump) = Pubkey::find_program_address(
            &[b"commons_token_mint", hatch_config.key().as_ref()],
            ctx.program_id,
        );
        hatch_config.commons_token_mint = ctx.accounts.commons_token_mint.key();
        hatch_config.commons_token_mint_bump = commons_token_mint_bump;
        hatch_config.curve_config = curve_config_key;
        hatch_config.curve_config_bump = curve_config_bump;
        hatch_config.reserve_vault = ctx.accounts.reserve_vault.key();
        hatch_config.commons_treasury = ctx.accounts.commons_treasury.key();
        hatch_config.failed = false;
        hatch_config.finalized = true;

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

        let cpi_accounts = Transfer {
            from: ctx.accounts.hatch_vault.to_account_info(),
            to: ctx.accounts.user_reserve_token_account.to_account_info(),
            authority: hatch_config.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let seeds = &[
            b"hatch_config",
            hatch_config.reserve_asset_mint.as_ref(),
            &[hatch_config.hatch_config_bump],
        ];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, contribution.amount)?;

        contribution.refunded = true;
        hatch_config.total_refunded = hatch_config
            .total_refunded
            .checked_add(contribution.amount)
            .ok_or(HatchError::RefundOverflow)?;

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

        let mint_amount = contribution.amount;
        require!(mint_amount > 0, HatchError::EmptyContribution);

        let commons_token_mint_key = ctx.accounts.commons_token_mint.key();
        let curve_seeds = &[
            b"curve_config",
            commons_token_mint_key.as_ref(),
            &[hatch_config.curve_config_bump],
        ];
        let signer_seeds = &[&curve_seeds[..]];

        let cpi_accounts = MintTo {
            mint: ctx.accounts.commons_token_mint.to_account_info(),
            to: ctx.accounts.user_commons_token_account.to_account_info(),
            authority: ctx.accounts.curve_config.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        token::mint_to(cpi_ctx, mint_amount)?;

        contribution.claimed = true;

        Ok(())
    }

    pub fn close_hatch(ctx: Context<CloseHatch>) -> Result<()> {
        let hatch_config = &mut ctx.accounts.hatch_config;

        // Check if the hatch failed
        if !hatch_config.failed {
            return err!(HatchError::HatchNotFailed);
        }

        require!(
            hatch_config.total_refunded == hatch_config.total_raised,
            HatchError::UnrefundedContributions
        );

        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeHatch<'info> {
    #[account(init, payer = authority, space = 8 + 32 + 8 + 8 + 8 + 8 + 32 + 8 + 1 + 1 + 1 + 32 + 1 + 32 + 1 + 32 + 1 + 32 + 32 + 8, seeds = [b"hatch_config", reserve_asset_mint.key().as_ref()], bump)]
    // Added PDAs for hatch vault + ABC state
    pub hatch_config: Account<'info, HatchConfig>,
    pub reserve_asset_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = authority,
        token::mint = reserve_asset_mint,
        token::authority = hatch_config,
        seeds = [b"hatch_vault", reserve_asset_mint.key().as_ref()],
        bump
    )]
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
        space = 8 + 8 + 1 + 1 + 8, // Added 1 byte for claimed and 8 bytes for max_allocation
        seeds = [b"contribution", authority.key().as_ref()],
        bump
    )]
    pub contribution: Account<'info, Contribution>,
    #[account(
        mut,
        seeds = [b"hatch_vault", hatch_config.reserve_asset_mint.as_ref()], // Updated seeds
        bump = hatch_config.hatch_vault_bump, // Use dedicated bump for hatch_vault
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
#[instruction(kappa: u64, exponent: u64, initial_price: u64, friction: u64)]
pub struct FinalizeHatch<'info> {
    #[account(mut)]
    pub hatch_config: Account<'info, HatchConfig>,
    #[account(mut, address = hatch_config.reserve_asset_mint)]
    pub reserve_asset_mint: Account<'info, Mint>,
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(mut)]
    pub curve_config: UncheckedAccount<'info>,
    #[account(
        init,
        payer = authority,
        mint::authority = curve_config,
        mint::decimals = 6,
        seeds = [b"commons_token_mint", hatch_config.key().as_ref()],
        bump,
    )]
    pub commons_token_mint: Account<'info, Mint>,
    /// CHECK: Created by the CPI in `commons_abc::initialize_curve`
    #[account(mut)]
    pub reserve_vault: UncheckedAccount<'info>,
    /// CHECK: Created by the CPI in `commons_abc::initialize_curve`
    #[account(mut)]
    pub commons_treasury: UncheckedAccount<'info>,
    pub commons_abc_program: Program<'info, CommonsAbc>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Refund<'info> {
    #[account(mut)]
    pub hatch_config: Account<'info, HatchConfig>,
    #[account(
        mut,
        seeds = [b"contribution", authority.key().as_ref()],
        bump
    )]
    pub contribution: Account<'info, Contribution>,
    #[account(
        mut,
        seeds = [b"hatch_vault", hatch_config.reserve_asset_mint.as_ref()],
        bump = hatch_config.hatch_vault_bump,
    )]
    pub hatch_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_reserve_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut)]
    pub hatch_config: Account<'info, HatchConfig>,
    #[account(
        mut,
        seeds = [b"contribution", authority.key().as_ref()],
        bump
    )]
    pub contribution: Account<'info, Contribution>,
    #[account(mut, address = hatch_config.commons_token_mint)]
    pub commons_token_mint: Account<'info, Mint>,
    #[account(
        mut,
        seeds = [b"curve_config", commons_token_mint.key().as_ref()],
        bump = hatch_config.curve_config_bump
    )]
    pub curve_config: Account<'info, CurveConfig>,
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = commons_token_mint,
        associated_token::authority = authority
    )]
    pub user_commons_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
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
    pub hatch_vault_bump: u8,
    pub commons_token_mint: Pubkey,
    pub commons_token_mint_bump: u8,
    pub curve_config: Pubkey,
    pub curve_config_bump: u8,
    pub commons_treasury: Pubkey,
    pub reserve_vault: Pubkey,
    pub total_refunded: u64,
}

#[account]
pub struct Contribution {
    pub amount: u64,
    pub max_allocation: u64,
    pub refunded: bool,
    pub claimed: bool, // Add this field
}

// Helper function to create a leaf from the contributor's public key and allowed allocation
fn get_leaf_from_contributor_and_allocation(contributor: &Pubkey, allocation: u64) -> [u8; 32] {
    let mut data = contributor.to_bytes().to_vec();
    data.extend_from_slice(&allocation.to_le_bytes());
    hash::hashv(&[&data]).to_bytes()
}

// Helper function to verify Merkle proof
fn verify_merkle_proof(proof: Vec<[u8; 32]>, root: [u8; 32], leaf: [u8; 32]) -> bool {
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
    #[msg("Allowed allocation mismatch.")]
    AllowedAllocationMismatch,
    #[msg("Allocation exceeded.")]
    AllocationExceeded,
    #[msg("Invalid allowed allocation.")]
    InvalidAllocation,
    #[msg("Invalid contribution amount.")]
    InvalidContributionAmount,
    #[msg("Total raise exceeds the maximum allowed.")]
    RaiseTooHigh,
    #[msg("Provided reserve mint does not match the hatch configuration.")]
    IncorrectReserveMint,
    #[msg("Curve config PDA does not match the commons ABC derivation.")]
    InvalidCurveConfig,
    #[msg("Refund total overflow.")]
    RefundOverflow,
    #[msg("Contribution amount is zero.")]
    EmptyContribution,
    #[msg("Not all contributions have been refunded.")]
    UnrefundedContributions,
}
