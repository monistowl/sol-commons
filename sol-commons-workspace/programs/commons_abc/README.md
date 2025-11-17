# Commons ABC

This program is responsible for the Augmented Bonding Curve (ABC) of the Commons Stack on Solana.

## Solana Design

### Accounts / PDAs:

*   `CurveConfigPda`
    *   Curve parameters (kappa, exponent, initial price, friction, etc.)
    *   Links to:
        *   `commons_token_mint`
        *   `reserve_mint` (e.g. USDC)
        *   `reserve_vault` (PDA-owned token account)
        *   `commons_treasury` (PDA-owned token account or Realms-managed)
*   `UserPosition` (optional; for stats/UX rather than strictly needed).
*   `Allowlist` (Merkle root or external attestor program).

### Instructions:

1.  `initialize_curve`
    *   Set parameters and create vault accounts.
    *   Seed with initial reserve & initial token supply (after Hatch).
2.  `buy_tokens`
    *   Inputs: amount of reserve to spend.
    *   Steps:
        *   Transfer reserve from user → `reserve_vault`.
        *   Compute how many Commons tokens to mint based on current supply & curve formula.
        *   Split inflow:
            *   `reserve_share` → remains in `reserve_vault`
            *   `common_pool_share` → move to `commons_treasury` via token transfer CPI.
            *   Tributes: optionally to fee sink or protocol treasury.
        *   Mint Commons tokens to user.
3.  `sell_tokens`
    *   Inputs: amount of Commons tokens to burn.
    *   Steps:
        *   Burn Commons tokens from user.
        *   Compute payout in reserve using inverse of curve.
        *   Apply exit tribute; transfer payout from `reserve_vault` → user.
        *   Tribute share → `commons_treasury`.
4.  `admin_update_params` (governance-gated)
    *   Change kappa, friction, etc., only via DAO decisions.

### Notes:

*   Use **Anchor** for account serialization & CPI to SPL Token.
*   For allowlist gating (trusted seed), integrate with `commons_hatch` or a separate membership token program (like CSTK equivalent).
*   All heavy math is done in Rust with fixed-point decimals (e.g. 64.64 or 32.32); we can port formulas from existing ABC spec.