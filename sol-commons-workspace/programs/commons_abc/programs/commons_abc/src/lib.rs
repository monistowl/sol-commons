use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Burn, Mint, MintTo, Token, TokenAccount, Transfer};
use core::convert::TryInto;
use spl_math::precise_number::PreciseNumber;
use spl_math::uint::U256;

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
        initial_reserve: u64,
        initial_supply: u64,
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

        let invariant = compute_invariant(initial_supply, initial_reserve, kappa)?;
        curve_config.invariant = precise_to_bytes(&invariant);
        Ok(())
    }

    pub fn buy_tokens(ctx: Context<BuyTokens>, amount: u64) -> Result<()> {
        let curve_config = &ctx.accounts.curve_config;
        let reserve_before = ctx.accounts.reserve_vault.amount;
        let (reserve_share, common_pool_share) =
            split_with_friction(amount, curve_config.friction)?;
        let reserve_after = reserve_before
            .checked_add(reserve_share)
            .ok_or(CommonsAbcError::MathOverflow)?;
        let minted_tokens = minted_tokens_for_deposit(reserve_before, reserve_after, curve_config)?;

        // Transfer reserve inflow to reserve_vault
        let transfer_accounts = Transfer {
            from: ctx.accounts.user_reserve_token_account.to_account_info(),
            to: ctx.accounts.reserve_vault.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                transfer_accounts,
            ),
            amount,
        )?;

        let bump = [curve_config.curve_config_bump];
        let seeds = [
            b"curve_config",
            curve_config.commons_token_mint.as_ref(),
            &bump,
        ];
        let signer = &[&seeds[..]];

        // Move common pool share from reserve vault to commons treasury
        if common_pool_share > 0 {
            let transfer_accounts = Transfer {
                from: ctx.accounts.reserve_vault.to_account_info(),
                to: ctx.accounts.commons_treasury.to_account_info(),
                authority: ctx.accounts.curve_config.to_account_info(),
            };
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    transfer_accounts,
                    signer,
                ),
                common_pool_share,
            )?;
        }

        // Mint commons tokens for buyer
        let mint_accounts = MintTo {
            mint: ctx.accounts.commons_token_mint.to_account_info(),
            to: ctx.accounts.user_commons_token_account.to_account_info(),
            authority: ctx.accounts.curve_config.to_account_info(),
        };
        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                mint_accounts,
                signer,
            ),
            minted_tokens,
        )?;

        Ok(())
    }

    pub fn sell_tokens(ctx: Context<SellTokens>, amount: u64) -> Result<()> {
        let curve_config = &ctx.accounts.curve_config;
        let supply_before = ctx.accounts.commons_token_mint.supply;
        let supply_after = supply_before
            .checked_sub(amount)
            .ok_or(CommonsAbcError::InsufficientSupply)?;
        let reserve_delta = reserve_delta_for_burn(supply_before, supply_after, curve_config)?;
        let exit_tribute = compute_fee(reserve_delta, curve_config.friction)?;
        let net_payout = reserve_delta
            .checked_sub(exit_tribute)
            .ok_or(CommonsAbcError::MathOverflow)?;
        if net_payout == 0 {
            return Err(CommonsAbcError::ZeroPayout.into());
        }
        let bump = [curve_config.curve_config_bump];
        let seeds = [
            b"curve_config",
            curve_config.commons_token_mint.as_ref(),
            &bump,
        ];
        let signer = &[&seeds[..]];

        let burn_accounts = Burn {
            mint: ctx.accounts.commons_token_mint.to_account_info(),
            from: ctx.accounts.user_commons_token_account.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        token::burn(
            CpiContext::new(ctx.accounts.token_program.to_account_info(), burn_accounts),
            amount,
        )?;

        let payout_accounts = Transfer {
            from: ctx.accounts.reserve_vault.to_account_info(),
            to: ctx.accounts.user_reserve_token_account.to_account_info(),
            authority: ctx.accounts.curve_config.to_account_info(),
        };
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                payout_accounts,
                signer,
            ),
            net_payout,
        )?;

        if exit_tribute > 0 {
            let tribute_accounts = Transfer {
                from: ctx.accounts.reserve_vault.to_account_info(),
                to: ctx.accounts.commons_treasury.to_account_info(),
                authority: ctx.accounts.curve_config.to_account_info(),
            };
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    tribute_accounts,
                    signer,
                ),
                exit_tribute,
            )?;
        }

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
#[instruction(kappa: u64, exponent: u64, initial_price: u64, friction: u64, initial_reserve: u64, initial_supply: u64)]
pub struct InitializeCurve<'info> {
    #[account(init, payer = authority, space = 8 + 32 + 160 + 1 + 32, seeds = [b"curve_config", commons_token_mint.key().as_ref()], bump)]
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
    pub invariant: [u8; 32],
}

const FEE_DENOMINATOR: u64 = 1_000_000;

fn split_with_friction(
    amount: u64,
    friction: u64,
) -> std::result::Result<(u64, u64), CommonsAbcError> {
    if friction > FEE_DENOMINATOR {
        return Err(CommonsAbcError::InvalidFriction);
    }
    let amount_precise = precise_from_u64(amount)?;
    let numerator = precise_from_u64(friction)?;
    let denominator = precise_from_u64(FEE_DENOMINATOR)?;
    let common_pool = amount_precise
        .checked_mul(&numerator)
        .and_then(|m| m.checked_div(&denominator))
        .ok_or(CommonsAbcError::MathOverflow)?;
    let reserve_share = amount_precise
        .checked_sub(&common_pool)
        .ok_or(CommonsAbcError::MathOverflow)?;
    Ok((
        precise_to_u64(&reserve_share)?,
        precise_to_u64(&common_pool)?,
    ))
}

fn compute_fee(amount: u64, friction: u64) -> std::result::Result<u64, CommonsAbcError> {
    let (_, pool) = split_with_friction(amount, friction)?;
    Ok(pool)
}

fn compute_invariant(
    initial_supply: u64,
    initial_reserve: u64,
    kappa: u64,
) -> std::result::Result<PreciseNumber, CommonsAbcError> {
    let supply = precise_from_u64(initial_supply)?;
    let reserve = precise_from_u64(initial_reserve)?;
    let supply_pow = supply
        .checked_pow(kappa as u128)
        .ok_or(CommonsAbcError::MathOverflow)?;
    supply_pow
        .checked_div(&reserve)
        .ok_or(CommonsAbcError::MathOverflow)
}

fn minted_tokens_for_deposit(
    reserve_before: u64,
    reserve_after: u64,
    config: &CurveConfig,
) -> std::result::Result<u64, CommonsAbcError> {
    let invariant = precise_from_bytes(&config.invariant)?;
    let supply_before =
        supply_from_reserve(precise_from_u64(reserve_before)?, &invariant, config.kappa)?;
    let supply_after =
        supply_from_reserve(precise_from_u64(reserve_after)?, &invariant, config.kappa)?;
    let minted = supply_after
        .checked_sub(&supply_before)
        .ok_or(CommonsAbcError::MathOverflow)?;
    let minted_u64 = precise_to_u64(&minted)?;
    if minted_u64 == 0 {
        return Err(CommonsAbcError::ZeroMint);
    }
    Ok(minted_u64)
}

fn reserve_delta_for_burn(
    supply_before: u64,
    supply_after: u64,
    config: &CurveConfig,
) -> std::result::Result<u64, CommonsAbcError> {
    let invariant = precise_from_bytes(&config.invariant)?;
    let supply_before_precise = precise_from_u64(supply_before)?;
    let supply_after_precise = precise_from_u64(supply_after)?;
    let reserve_before = reserve_from_supply(&supply_before_precise, &invariant, config.kappa)?;
    let reserve_after = reserve_from_supply(&supply_after_precise, &invariant, config.kappa)?;
    let delta = reserve_before
        .checked_sub(&reserve_after)
        .ok_or(CommonsAbcError::MathOverflow)?;
    let delta_u64 = precise_to_u64(&delta)?;
    if delta_u64 == 0 {
        return Err(CommonsAbcError::ZeroPayout);
    }
    Ok(delta_u64)
}

fn supply_from_reserve(
    reserve: PreciseNumber,
    invariant: &PreciseNumber,
    kappa: u64,
) -> std::result::Result<PreciseNumber, CommonsAbcError> {
    let product = invariant
        .checked_mul(&reserve)
        .ok_or(CommonsAbcError::MathOverflow)?;
    nth_root(&product, kappa)
}

fn reserve_from_supply(
    supply: &PreciseNumber,
    invariant: &PreciseNumber,
    kappa: u64,
) -> std::result::Result<PreciseNumber, CommonsAbcError> {
    let supply_pow = supply
        .checked_pow(kappa as u128)
        .ok_or(CommonsAbcError::MathOverflow)?;
    supply_pow
        .checked_div(invariant)
        .ok_or(CommonsAbcError::MathOverflow)
}

fn nth_root(
    value: &PreciseNumber,
    root: u64,
) -> std::result::Result<PreciseNumber, CommonsAbcError> {
    if value.value.is_zero() {
        return Ok(PreciseNumber {
            value: U256::zero(),
        });
    }
    let mut low = U256::zero();
    let mut high = value.value;
    let one = U256::one();
    while high > low {
        let mid = (low + high + one) >> 1;
        let mid_precise = PreciseNumber { value: mid };
        let mid_pow = mid_precise
            .checked_pow(root as u128)
            .ok_or(CommonsAbcError::MathOverflow)?;
        if mid_pow.value <= value.value {
            low = mid;
        } else if mid == U256::zero() {
            break;
        } else {
            high = mid - one;
        }
    }
    Ok(PreciseNumber { value: low })
}

fn precise_from_u64(value: u64) -> std::result::Result<PreciseNumber, CommonsAbcError> {
    PreciseNumber::new(value as u128).ok_or(CommonsAbcError::MathOverflow)
}

fn precise_from_bytes(bytes: &[u8; 32]) -> std::result::Result<PreciseNumber, CommonsAbcError> {
    let mut arr = [0u8; 32];
    arr.copy_from_slice(bytes);
    Ok(PreciseNumber {
        value: U256::from_little_endian(&arr),
    })
}

fn precise_to_bytes(value: &PreciseNumber) -> [u8; 32] {
    value.value.to_little_endian()
}

fn precise_to_u64(value: &PreciseNumber) -> std::result::Result<u64, CommonsAbcError> {
    let imprecise = value.to_imprecise().ok_or(CommonsAbcError::MathOverflow)?;
    imprecise
        .try_into()
        .map_err(|_| CommonsAbcError::MathOverflow)
}

#[error_code]
pub enum CommonsAbcError {
    #[msg("Arithmetic overflow in curve math.")]
    MathOverflow,
    #[msg("Mint amount is zero.")]
    ZeroMint,
    #[msg("Payout is zero.")]
    ZeroPayout,
    #[msg("Not enough supply to burn.")]
    InsufficientSupply,
    #[msg("Friction parameter exceeds 100%.")]
    InvalidFriction,
}
