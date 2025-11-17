## Sol Commons on-chain toolkit

This repository brings the **Commons Stack** primitives to Solana: an **Augmented Bonding Curve (ABC)** that continuously funds a community treasury and **Conviction Voting** that lets token holders signal their support for funding requests over time. The design mirrors how the Commons Stack combines reserve/funding pools with conviction‑driven proposal execution to keep “impact = profit” for public goods projects.citeturn0search0

### Core concepts

- **Augmented Bonding Curve (ABC)** – buyers mint tokens by depositing reserve assets into a curve that allocates inflows between a long-term Reserve Pool and an immediately spendable Funding Pool; hatch participants receive slowly unlocking tokens tied to capital allocations, and every burn routes a tribute back into the Funding Pool so the commons regains value from exits.citeturn0search0
- **Conviction Voting** – continuous, time-weighted signaling where each supporter’s conviction grows while they keep their vote unchanged, enabling a social sensor fusion signal that triggers funding once accumulated conviction passes an adaptive threshold.citeturn0search2turn0search4
- **Commons Assembly** – the ABC and Conviction Voting components sit together as the core economic & governance stack that feeds downstream apps (e.g., Giveth’s request funding engine) so communities can coordinate and spend from a commons treasury.citeturn0search2

### Current implementation status

1. **`commons_hatch` & `commons_abc`** – Anchor programs that bootstrap funding (with Merkle allowlists, contributions, and refunds) and mirror the ABC formula. `commons_hatch` initializes the curve via CPI into `commons_abc`, which tracks a curve invariant, reserve/vault accounts, and friction/fund-split math. Integration tests already cover mint/claim/refund flows.
2. **`commons_conviction_voting`** – now enforces proposal state transitions, manages a staking vault, updates conviction via decay/threshold functions, and gates treasury transfers until required conviction is reached. Unit tests validate decay, required‑conviction math, and staking math.
3. **`commons_rewards`** – provides a Merkle-backed reward epoch and payout flow (needs the PDA signer fix noted in issue `sol-commons-dvi`).
4. **Off-chain services** – `offchain/praise-service`, `offchain/tokenlog-service`, and `offchain/simulator-pipeline` are placeholders; follow-up work (issue `sol-commons-2bh`) should replace the READMEs with concrete pipelines or docs.

### Getting started

1. Install Anchor and run `yarn` under `sol-commons-workspace` to sync the TypeScript tooling.
2. `cd sol-commons-workspace && cargo test -p commons_hatch`/`cargo test -p commons_abc`/`cargo test -p commons_conviction_voting` to exercise on-chain units.
3. Build/deploy via Anchor CLI, wiring the PDAs described in each program (curve_config, staking_vault, reward_epoch, etc.).

### Next milestones

- Finish Conviction Voting integration & treasury wiring so proposals can drain the ABC-funded commons (issue `sol-commons-1s3`).
- Repair the reward PDA signer (issue `sol-commons-dvi`) and harden the hatch window/gating logic (issue `sol-commons-v17`).
- Expand README placeholders within `offchain/` into runnable services or spec docs as part of `sol-commons-2bh`.

