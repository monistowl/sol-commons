# Sol Commons on-chain toolkit

**Sol Commons** ports the analytical Commons Stack toolkit ([commonsstack.org](https://commonsstack.org)) into Solana.

The goal: make it easy for any community to launch a **regenerative, self-governing micro-economy** around a shared cause—open source software, local climate projects, research, mutual aid, you name it. Instead of one-off grants and donation drives, you get a continuously funded treasury and a governance system that rewards patient, aligned contributors.

Traditional tools fall short:

- Simple treasuries run out of money or create endless fundraising overhead.
- Vanilla bonding curves offer liquidity, but tend to reward fast speculation more than long-term stewardship.
- Snapshot token voting is brittle—turnout is low, whales can dominate, and decision-making comes in short, stressful bursts.

The Commons Stack pattern tackles this by tightly coupling three pieces into one loop:

1. **Augmented Bonding Curve (ABC)** – a primary-issuance AMM that:
   - Mints and burns tokens along a programmable curve.
   - Splits every contribution into a **Reserve Pool** (for predictable liquidity) and a **Funding Pool** (a commons treasury the community can spend).
   - Charges a small **tribute** on exits, sending a slice of each sell back into the Funding Pool so the commons benefits even when traders cash out.

   In practice: your community token is liquid from day one, but the system is constantly topping up a shared treasury instead of leaking value out of the commons.

2. **Conviction Voting** – a continuous decision-making mechanism for allocating that treasury:
   - Token holders **lock tokens behind proposals** they want to see funded.
   - Their support (conviction) **grows over time** as they stay staked, and decays when they move their tokens.
   - Each proposal has an **adaptive threshold** based on how much it’s asking for relative to the treasury, so small ideas can pass quickly while big spends need deep, sustained backing.

   This turns governance into a **steady, time-weighted signal** instead of a series of rushed “voting days.” Long-term, aligned participants gain more influence than short-term speculators, and proposals only execute once they’ve earned enough conviction to justify the spend.

3. **Off-chain assembly / connected services** – the cultural and data layer that drives the on-chain machinery:
   - **Praise** captures and quantifies community recognition, then batches it into Merkle trees for reward epochs.
   - **Tokenlog** ties GitHub issues and wallet balances together, so work queues and stakeholders are visible in one place.
   - A **Simulator** lets you sweep through different curve and governance parameters off-chain and generate parameter kits before deploying anything on-chain.

   Together, these services turn messy human inputs—contributions, code, coordination—into deterministic data that can feed the ABC and Conviction Voting modules.

**Sol Commons** implements that full pattern on Solana with Anchor programs and a reproducible off-chain pipeline. You get:

- A **hatch** to bring in trusted early contributors, set initial parameters, and bootstrap the curve.
- An **ABC** that continuously mints/burns commons tokens, keeps liquidity in a reserve, and regeneratively funds the treasury.
- A **Conviction Voting** program that guards the treasury so funds only move when proposals accumulate enough conviction.
- A **Merkle-based rewards system** and TypeScript integration tests tying Praise/Tokenlog/Simulator outputs into on-chain reward epochs.

If you want to launch a Solana-native commons that can both **fund itself** and **govern itself** in a principled, continuous way, this repo is the on-chain toolkit you plug into.

---

## What this repo provides

| Layer | Description |
|---|---|
| `commons_hatch`, `commons_abc` | Anchor programs that open a Merkle-gated hatch, settle contributions in a vault, and initialize the ABC curve via CPI. The curve tracks a 3-account setup (curve_config PDA, reserve vault, treasury), handles friction splits, and supports refund/claim flows (see existing tests for minted, refunded, and claimed scenarios). |
| `commons_conviction_voting` | Converts the classic CV pattern into Anchor: stakes flow through a global staking vault/PDA, every stake/unstake updates conviction with decay, a helper computes the required conviction threshold, and execution is blocked until the treasury-target ratio is satisfied. Tests cover math correctness and the lifecycle. |
| `commons_rewards` | Merkle-based reward epochs with claim replay protection—every epoch stores its vault + PDA bumps, enforces Merkle proofs, and now records claimed leaves so duplicates fail with `AlreadyClaimed`. |
| Off-chain services | Praise, Tokenlog, and Simulator scaffolds provide deterministic data: Praise emits Merkle roots + proofs, Tokenlog fetches GitHub issues before failing back to mocks, and the Simulator provides curve scenarios + metrics so the Mocha/Anchor suites recreate the same parameters used in deployment. |
| Integration tests | `tests/offchain-integration.js` validates the off-chain pipeline; `scripts/run-offchain-with-validator.sh` runs those Mocha tests against `solana-test-validator`; `tests/sol-commons-workspace.ts` shows how the Merkle batch funds a `commons_rewards` epoch, while `tests/commons_conviction_voting/conviction.rs` now ensures Treasury transfers only happen via the CV PDA under the right thresholds. |

---

## Example scenarios

1. **Community Hatch + ABC deployment**  
   The community runs `commons_hatch.initialize` with parameters, an allowlisted Merkle root, and a reserve mint. Contributors deposit during the open window; the finalizer uses `commons_abc.initialize_curve` to mint treasury and reserve vaults, then contributors can claim tokens post-hatch.

2. **Continuous funding via conviction**  
   Commons token holders stake into proposals via `commons_conviction_voting.stake_tokens`. `update_conviction_for_proposal` keeps a decayed conviction value, and only when the calculated threshold is met does `check_and_execute` approve the request and pay out from the treasury via the CV PDA authority.

3. **Praise → reward epoch**  
   The off-chain Praise service collects kudos, builds a Merkle batch, and `tests/sol-commons-workspace.ts` demonstrates how that batch becomes a `commons_rewards` epoch tied to a PDA vault with a deterministic mint. Claimers can later pull funds from the vault using the proof generated by the same off-chain service.

---

## Off-chain pipeline & PDA mapping

`offchain/pipeline/index.js` gathers the Praise, Tokenlog, and Simulator scaffolds into one deterministic payload. It can inject custom praise events, fetch GitHub issues and balance snapshots, and run simulated configuration sweeps so the JSON it emits mirrors the inputs that the on-chain programs expect:

- **Praise + Merkle batch**: `batch` contains the Merkle root, total tokens, and claim list used by `commons_rewards.create_reward_epoch`. `proofs` map each wallet to the leaf proof so downstream tooling can compose valid `claim_reward` transactions.
- **Tokenlog**: `issues` and `balances` come straight from `offchain/tokenlog-service`, providing the DAO with GitHub priorities plus wallet snapshots that could drive CV parameter proposals.
- **Simulator**: `simulation` exposes the tuned `fundingRatio`, `convictionDecay`, and scenario metrics that CLI tools can feed into `commons_abc`/`commons_conviction_voting` initialization instructions.

The Merkle batch ties to the on-chain PDAs as follows:

1. `RewardEpochPda` uses `[b"reward_epoch", epoch_id.to_le_bytes()]` to store the epoch metadata (Merkle root, total tokens, vault bumps).
2. `reward_vault` and `reward_vault_authority` are PDAs seeded with `[b"reward_vault", epoch_id.to_le_bytes()]` and `[b"reward_vault_authority", epoch_id.to_le_bytes()]`, making the vault signer deterministic and replay-safe.
3. Each claimer seeds `[b"reward_claim", epoch_id.to_le_bytes(), claimer_pubkey]` for a `ClaimStatus` account that records whether the proof has already been used, preventing duplicate claims.

TypeScript and Mocha integration shims (`sol-commons-workspace/tests/sol-commons-workspace.ts` and `tests/offchain-integration.js`) demonstrate the flow: the payload deposits tokens into the PDA vault, stores the Merkle root, then uses the proofs to call `claim_reward`, all while respecting the PDA derivations above. `scripts/run-offchain-with-validator.sh` now runs the pipeline before launching `solana-test-validator`, writes the JSON snapshot to disk, and exposes it to the tests via `OFFCHAIN_PIPELINE_PAYLOAD` so the validator suite reuses the same deterministic data (including the claimer event) as the standalone Mocha run.

For production tooling, `yarn pipeline:cli` runs `sol-commons-workspace/cli/emit-reward-batch.ts`, which calls the same pipeline, derives the reward/vault/claim PDAs, and prints the instruction-ready payload that TypeScript clients or CLIs can send to `commons_rewards.create_reward_epoch` and `claim_reward` when posting Merkle roots to a deployed epoch. Run `node offchain/pipeline/index.js` for quick debugging or `yarn pipeline:cli --output ./tmp/paylod.json` to capture a ready-to-send batch.

---

## How to get started

1. Install Anchor and run `yarn` in `sol-commons-workspace`; the workspace already includes `@coral-xyz/anchor`, `@solana/spl-token`, and the off-chain scaffolds plus the generated `commons_rewards` IDL under `offchain/commons_rewards.idl.json`.
2. Start a local validator (`solana-test-validator`) before running Anchor tests so the new off-chain inputs can be validated, then run `yarn test:offchain` and `yarn test:offchain-validator` (the script rewrites `ANCHOR_PROVIDER_URL` and runs the Mocha suite under the validator) to exercise both the off-chain Merkle flow and the guarded conviction/treasury paths plus `cargo test -p commons_hatch`/`commons_abc`/`commons_conviction_voting`/`commons_rewards`.
3. Use `Anchor.toml` + `cargo test` to deploy locally. Point `offchain/*/config.json` to the deployed programs and replay the on-chain flows via the integration tests once `solana-test-validator` is running.

---

## Staying aligned

- `sol-commons-v17`: hatch gating is in place, but consider adding more integration tests to prove contributions obey slot windows and finalization states.
- `sol-commons-2bh`: the scaffolds & integration tests exist, so this issue now becomes implementing real services (e.g., praise aggregator, GitHub tokenlog bot, cadCAD simulator) that exercise the Anchor tests end-to-end.

---

## Tests

```bash
yarn test:offchain
cargo test -p commons_hatch
cargo test -p commons_abc
cargo test -p commons_conviction_voting
cargo test -p commons_rewards
cd sol-commons-workspace && yarn test:offchain-validator
```

## Full-lifecycle validation

`sol-commons-workspace/tests/full-lifecycle.ts` now strings the hatch → ABC → conviction voting → rewards flow together with the off-chain payload generator, so it is the fastest way to validate the entire pipeline in one shot. To run that scenario:

1. Start `solana-test-validator --reset --quiet` (Anchor expects the default RPC at `http://127.0.0.1:8899`).
2. Export `ANCHOR_PROVIDER_URL=http://127.0.0.1:8899` (and `ANCHOR_WALLET` if you point Anchor at a specific keypair) so `AnchorProvider.env()` can connect.
3. Run `yarn run ts-mocha -p ./tsconfig.json -t 1000000 tests/full-lifecycle.ts`. You can also set `OFFCHAIN_PIPELINE_PAYLOAD` to reuse a previously written payload instead of rebuilding the Praise/Simulator assets every time.

Because it asserts each PDA derivation and CPI call in sequence, this test is useful for smoke-checking the entire implementation before running the per-program Rust suites.
