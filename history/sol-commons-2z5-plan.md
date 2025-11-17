# sol-commons-2z5: commons_hatch Phase‑1 planning

## Context
- `sol-commons-workspace/programs/commons_hatch/programs/commons_hatch/src/lib.rs` initializes `hatch_vault` as a normal token account in `InitializeHatch`, but every CPI (refund path and later sell/claim flows) treats it as a PDA derived from `["hatch_vault", reserve_mint]`; missing the PDA means the CPI signer has no account to sign for, so the hatch cannot finalize or refund successfully.
- The Merkle gate set in `contribute` currently hashes `contributor || amount`, meaning a single leaf only locks in a specific allocation; contributors can therefore submit proofs with arbitrary amounts once they know the hash format. We need to split identity vs. allocation metadata so the gate enforces `contributor` and the allocation is enforced separately (signed metadata, per-stage leaves, or stored allocation limits).
- `finalize`, `claim`, `refund`, and `close` are full of TODOs (minting ABC tokens, enforcing raise caps, refund guarantees, checking `contribution.claimed`/`refunded`, instantiating `commons_abc`, etc.), so there is no real lifecycle logic or tests. Without these the roadmap’s Phase‑1 is just scaffolding.

## Goals
1. Make `hatch_vault` a PDA so every CPI-path (refund, claim, finalize) can sign via Anchor without hitting “missing or invalid signer” errors.
2. Harden the Merkle gate so proofs are contributor-centric and deposit limits are enforced via signed metadata or stored allowances, preventing forged allocations.
3. Implement finalize/refund/claim/close flows that instantiate `commons_abc`, respect min/max raises, mint commons tokens, and guarantee refunds before closing, instead of leaving TODO stubs.
4. Provide Anchor tests that cover happy/failure hatch outcomes: contributions before and after open/close slots, merkle proof rejection, finalize+claim success, and refund path when min raise fails.

## Implementation steps
1. Update `InitializeHatch` to derive `hatch_vault` with `#[account(... seeds = [b"hatch_vault", reserve_asset_mint.key().as_ref()], bump)]` or equivalent, store the bump on `HatchConfig`, and ensure `hatch_config.hatch_vault` references the PDA. Keep rent-initialized token account logic or replace with `init_if_needed` + `mint`/`authority` settings to support PDA control.
2. Adjust the `Contribute` account constraints to `seeds = [b"hatch_vault", hatch_config.reserve_asset_mint.as_ref()]` with the stored bump and remove the existing token account init path; the PDA should exist before the first contribution to accept CPI transfers.
3. Refactor `get_leaf_from_contributor`/proof verification so the Merkle tree hashes only contributor identity; store per-contributor allocations (maybe in `Contribution` or a new account) that are enforced when calling `contribute` (or require signed metadata/allowlist per stage). Document how allocations are bound to the tree/metadata and validate amounts before transferring.
4. Expand `FinalizeHatch` to: verify `total_raised` lies between `min_raise` and `max_raise`, derive the commons token mint PDA, call `commons_abc::initialize_curve` (CPI) to create the curve config + vaults, and record the curve/mint PDAs in `HatchConfig` so later instructions can mint via the curve authority. Keep the raise checks and mark `finalized` only after the CPI succeeds.
5. Harden `Claim`/`Refund`/`Close`: `Claim` will mint commons tokens from the stored curve PDA authority using the recorded commons mint, `Refund` should guard against double refunds via `contribution.refunded` while signing with the hatch config PDA, and `Close` must check that `total_refunded == total_raised` before closing so the vault can be reclaimed. Each path should record state transitions so tests can assert them.
6. Introduce Anchor tests in `sol-commons-workspace/programs/commons_hatch/tests`: contributions with/without proofs, finalize success/failure, claim success (minting via the curve PDA), refund when failed, and `close_hatch` guard. Use deterministic slot advancement or Anchor clock mocks to exercise open/close windows.

## Progress
- Added `commons_abc` CPI wiring in `FinalizeHatch` along with PDA tracking for the commons mint and curve config so later instructions can mint and validate PDAs.
- Hardened `Claim` so it mints through the curve config PDA, and introduced integration tests (`tests/commons_hatch.rs`) for both happy and unhappy hatch outcomes.
- Re-aligned the workspace/dependencies (`sol-commons-workspace/Cargo.toml`, the `commons_abc` CPI feature, dev dependencies, and lock files) so `cargo test` now runs cleanly with the `solana-sdk` 2.3 stack, proving the Phase‑1 flows end-to-end.
- Implemented the actual ABC math via `spl-math` (with `no-entrypoint`), computed the reserve/supply invariant with friction splits, added the supporting helper errors, and validated the buy/sell flows through a dedicated deterministic integration test harness plus helper-level checks.

## Risks / open questions
- Need to coordinate with `commons_abc` on CPI seeds, curve config initialization, and token mints before finalizing; Phase-1 doc indicates ABC math is still pending, so we may need to stub CPI until `sol-commons-3rl` lands (just ensure interfaces align).
- Merkle tree rework may require data format decision for off-chain Praise/Tokenlog data; record the format in this plan so future Phase-2 work can hook into the same leaves.
- Ensure claim/refund flows cannot race other instructions: gating on `hatch_config.finalized`, `contribution.claimed`, and `contribution.refunded` is mandatory.
