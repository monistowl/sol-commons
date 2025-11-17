# Commons Hatch

This program is responsible for the Hatch phase of the Commons Stack on Solana.

## Solana Design

### Accounts / PDAs:

*   `HatchConfigPda`
    *   Reserve asset mint, min\_raise, max\_raise, open/close slots.
    *   Merkle root for trusted seed allowlist.
    *   Pointer to final `CurveConfigPda` template & governance config.
*   `ContributionPda (user)`
    *   Tracks contributed amount, refunded flag.
*   `HatchVault` – PDA token account holding contributions.

### Instructions:

1.  `initialize_hatch`
    *   Set parameters, time window, Merkle root.
2.  `contribute`
    *   Verify user inclusion via Merkle proof, or via prior membership token.
    *   Transfer reserve tokens from user → `HatchVault`.
    *   Update `ContributionPda`.
3.  `finalize_hatch`
    *   After close\_slot:
        *   If total\_contributed < min\_raise → mark failed.
            *   Allow `refund` calls.
        *   Else:
            *   Define final ABC parameters (can be pre-computed by Simulator).
            *   Initialize `commons_token_mint`.
            *   Initialize `commons_abc` with:
                *   `reserve_vault` seeded from `HatchVault` per ABC design (some share to reserve, some to common pool).
            *   Mint Commons tokens:
                *   To contributors (pro-rata).
                *   To a “reward pool” and other stakeholders.
            *   Instantiate DAO (Realms) with Commons token as governance token.
4.  `refund`
    *   If hatch failed, let contributors withdraw their exact contribution.
5.  `claim_tokens`
    *   If hatch succeeded and vesting schedules apply, allow claiming over time.

### Notes:

*   You can keep Hatch and ABC as separate programs or have Hatch CPI into the ABC program once conditions are met.
*   There’s already a conceptual spec from TEC’s Hatch process; you’re just porting it to Solana’s account model.