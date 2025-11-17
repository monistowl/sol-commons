use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer, MintTo, Burn};
use anchor_spl::associated_token::AssociatedToken;

declare_id!("2xnNJU6bK1R6WvnBUmUKxftMyVuvXXhn3Vs5hDHM3KQv");

#[program]
pub mod commons_abc {
    use super::*;

    pub fn initialize_curve(
        ctx: Context<InitializeCurve>,
        kappa: u64,
        exponent: u64,
        initial_price: u64,
        friction: u64,
    ) -> Result<()> {
        let curve_config = &mut ctx.accounts.curve_config;
        curve_config.kappa = kappa;
        curve_config.exponent = exponent;
        curve_config.initial_price = initial_price;
        curve_config.friction = friction;
        curve_config.commons_token_mint = ctx.accounts.commons_token_mint.key();
        curve_config.reserve_mint = ctx.accounts.reserve_mint.key();
        curve_config.reserve_vault = ctx.accounts.reserve_vault.key();
        curve_config.commons_treasury = ctx.accounts.commons_treasury.key();
        curve_config.curve_config_bump = ctx.bumps.curve_config;
        curve_config.authority = ctx.accounts.authority.key(); // Store the authority
        Ok(())
    }

    pub fn buy_tokens(ctx: Context<BuyTokens>, amount: u64) -> Result<()> {
        let curve_config = &mut ctx.accounts.curve_config;

        // Transfer reserve from user to reserve_vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_reserve_token_account.to_account_info(),
            to: ctx.accounts.reserve_vault.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // TODO: Compute how many Commons tokens to mint based on current supply & curve formula.
        let commons_to_mint = amount; // Placeholder

        // Clone necessary fields from curve_config for CPI calls
        let curve_config_commons_token_mint = curve_config.commons_token_mint;
        let curve_config_bump = curve_config.curve_config_bump;

        // TODO: Split inflow: reserve_share and common_pool_share
        let reserve_share = amount; // Placeholder
        let common_pool_share = 0; // Placeholder

        // Transfer common_pool_share to commons_treasury
        let cpi_accounts = Transfer {
            from: ctx.accounts.reserve_vault.to_account_info(),
            to: ctx.accounts.commons_treasury.to_account_info(),
            authority: ctx.accounts.curve_config.to_account_info(), // PDA authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let seeds = &[
            b"curve_config",
            curve_config_commons_token_mint.as_ref(),
            &[curve_config_bump],
        ];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, common_pool_share)?;

        // Mint Commons tokens to user
        let cpi_accounts = MintTo {
            mint: ctx.accounts.commons_token_mint.to_account_info(),
            to: ctx.accounts.user_commons_token_account.to_account_info(),
            authority: ctx.accounts.curve_config.to_account_info(), // PDA authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let seeds = &[
            b"curve_config",
            curve_config_commons_token_mint.as_ref(),
            &[curve_config_bump],
        ];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::mint_to(cpi_ctx, commons_to_mint)?;

        Ok(())
    }

    pub fn sell_tokens(ctx: Context<SellTokens>, amount: u64) -> Result<()> {
        let curve_config = &mut ctx.accounts.curve_config;

        // Burn Commons tokens from user
        let cpi_accounts = Burn {
            mint: ctx.accounts.commons_token_mint.to_account_info(),
            from: ctx.accounts.user_commons_token_account.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::burn(cpi_ctx, amount)?;

        // TODO: Compute payout in reserve using inverse of curve.
        let reserve_payout = amount; // Placeholder

        // TODO: Apply exit tribute; transfer payout from reserve_vault to user.
        let exit_tribute = 0; // Placeholder
        let net_payout = reserve_payout - exit_tribute;

        // Clone necessary fields from curve_config for CPI calls
        let curve_config_reserve_mint = curve_config.reserve_mint;
        let curve_config_bump = curve_config.curve_config_bump;

        // Transfer payout from reserve_vault to user
        let cpi_accounts = Transfer {
            from: ctx.accounts.reserve_vault.to_account_info(),
            to: ctx.accounts.user_reserve_token_account.to_account_info(),
            authority: ctx.accounts.curve_config.to_account_info(), // PDA authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let seeds = &[
            b"curve_config",
            curve_config_reserve_mint.as_ref(),
            &[curve_config_bump],
        ];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, net_payout)?;

        // TODO: Tribute share to commons_treasury.
        let tribute_share = exit_tribute; // Placeholder

        // Transfer tribute_share to commons_treasury
        let cpi_accounts = Transfer {
            from: ctx.accounts.reserve_vault.to_account_info(),
            to: ctx.accounts.commons_treasury.to_account_info(),
            authority: ctx.accounts.curve_config.to_account_info(), // PDA authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let seeds = &[
            b"curve_config",
            curve_config_reserve_mint.as_ref(),
            &[curve_config_bump],
        ];
        let signer = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, tribute_share)?;

        Ok(())
    }

    pub fn admin_update_params(
        ctx: Context<AdminUpdateParams>,
        kappa: Option<u64>,
        exponent: Option<u64>,
        initial_price: Option<u64>,
        friction: Option<u64>,
    ) -> Result<()> {
        let curve_config = &mut ctx.accounts.curve_config;

        if let Some(kappa) = kappa {
            curve_config.kappa = kappa;
        }
        if let Some(exponent) = exponent {
            curve_config.exponent = exponent;
        }
        if let Some(initial_price) = initial_price {
            curve_config.initial_price = initial_price;
        }
        if let Some(friction) = friction {
            curve_config.friction = friction;
        }

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(kappa: u64, exponent: u64, initial_price: u64, friction: u64)]
pub struct InitializeCurve<'info> {
    #[account(init, payer = authority, space = 8 + 8 + 8 + 8 + 8 + 32 + 32 + 32 + 32 + 1 + 32, seeds = [b"curve_config", commons_token_mint.key().as_ref()], bump)] // Added 32 bytes for authority
    pub curve_config: Account<'info, CurveConfig>,
    pub commons_token_mint: Account<'info, Mint>,
    pub reserve_mint: Account<'info, Mint>,
    #[account(init, payer = authority, token::mint = reserve_mint, token::authority = curve_config)]
    pub reserve_vault: Account<'info, TokenAccount>,
    #[account(init, payer = authority, token::mint = commons_token_mint, token::authority = curve_config)]
    pub commons_treasury: Account<'info, TokenAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct BuyTokens<'info> {
    #[account(mut)]
    pub curve_config: Account<'info, CurveConfig>,
    #[account(mut)]
    pub commons_token_mint: Account<'info, Mint>,
    #[account(mut)]
    pub reserve_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub commons_treasury: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_reserve_token_account: Account<'info, TokenAccount>,
    #[account(init_if_needed, payer = authority, associated_token::mint = commons_token_mint, associated_token::authority = authority)]
    pub user_commons_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct SellTokens<'info> {
    #[account(mut)]
    pub curve_config: Account<'info, CurveConfig>,
    #[account(mut)]
    pub commons_token_mint: Account<'info, Mint>,
    #[account(mut)]
    pub reserve_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub commons_treasury: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_reserve_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_commons_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AdminUpdateParams<'info> {
    #[account(mut, has_one = authority)]
    pub curve_config: Account<'info, CurveConfig>,
    pub authority: Signer<'info>,
}

#[account]
pub struct CurveConfig {
    pub kappa: u64,
    pub exponent: u64,
    pub initial_price: u64,
    pub friction: u64,
    pub commons_token_mint: Pubkey,
    pub reserve_mint: Pubkey,
    pub reserve_vault: Pubkey,
    pub commons_treasury: Pubkey,
    pub curve_config_bump: u8,
    pub authority: Pubkey, // Add this field
}