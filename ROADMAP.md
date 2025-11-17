Here’s a concrete mapping of the Commons Stack toolkit to Solana, and a phased plan for actually building it.

---

## 1. What we’re porting, in practice

From the Commons Stack “technical toolkit” page, the core building blocks are:([Commons Stack][1])

* **Augmented Bonding Curve (ABC)** – a bonding-curve market maker that splits inflows/outflows between a reserve and a “common pool” treasury, with allowlists and tributes.([Commons Stack][1])
* **Conviction Voting (CV)** – continuous token staking behind proposals, where “conviction” accumulates over time until a threshold is met.([Commons Stack][1])
* **Hatch** – an initialization/IDO phase that bootstraps the Commons with a trusted seed allowlist, a minimum raise, and then instantiates the ABC + DAO if successful.([Commons Stack][1])
* **Praise** – off-chain community recognition → periodically translated into token or reputation rewards.([Commons Stack][1])
* **Tokenlog** – token-weighted prioritization of GitHub issues / parameters.([Commons Stack][1])
* **Quadratic Rank Choice Voting (QRCV)** – quadratic, ranked voting mode (spec still evolving).([Commons Stack][1])
* **Commons Simulator** – cadCAD-based simulation used to co-design parameters before deployment.([Commons Stack][1])

On Ethereum these are mostly Aragon/xDAI apps, EVM contracts, and off-chain services. On Solana we’re rebuilding the on-chain pieces as **SPL-compatible programs** and treating Praise/Tokenlog/Simulator as off-chain microservices integrated via wallets & signatures.

---

## 2. High-level Solana architecture

**Core primitives on Solana:**

* **Commons Token** – an SPL (or token-2022) fungible token mint.
* **Reserve Asset** – USDC, native SOL-wrapped, or other SPL stable.
* **Programs** (preferably Anchor):

  * `commons_hatch`
  * `commons_abc`
  * `commons_conviction_voting`
  * `commons_qrcv` (optional, later)
  * `commons_rewards` (Praise payouts & misc. distributions)
* **State** stored in PDAs:

  * Curve config & vaults
  * Proposals & stakes
  * Hatch contributions & vesting schedules
  * Reward epochs & allocations
* **Governance**:

  * Either reuse **SPL Governance / Realms** or write a light DAO wrapper that delegates certain actions (spending, param changes) to CV/QRCV outcomes.

**Off-chain services:**

* **Praise service** (Discord/Telegram integrations + DB + payout job).
* **Tokenlog-style service** (GitHub issues + on-chain balance snapshots).
* **Simulation pipeline** (cadCAD or Rust equivalent) to output config JSON → fed into Solana deployment CLIs.

---

## 3. Component-by-component design on Solana

### 3.1 Augmented Bonding Curve (ABC) on Solana

**What it does on Ethereum**

ABC is a bonding-curve AMM that:([Commons Stack][1])

* Mints/burns the Commons token along a predefined curve.
* Splits buys/sells into:

  * **Reserve Pool** (for liquidity guarantees)
  * **Common Pool / Treasury** (continuous funding)
* Adds extras: allowlist/trusted seed, entry/exit tributes, vesting.

**Solana design**

**Accounts / PDAs:**

* `CurveConfigPda`

  * Curve parameters (kappa, exponent, initial price, friction, etc.)
  * Links to:

    * `commons_token_mint`
    * `reserve_mint` (e.g. USDC)
    * `reserve_vault` (PDA-owned token account)
    * `commons_treasury` (PDA-owned token account or Realms-managed)
* `UserPosition` (optional; for stats/UX rather than strictly needed).
* `Allowlist` (Merkle root or external attestor program).

**Instructions:**

1. `initialize_curve`

   * Set parameters and create vault accounts.
   * Seed with initial reserve & initial token supply (after Hatch).

2. `buy_tokens`

   * Inputs: amount of reserve to spend.
   * Steps:

     * Transfer reserve from user → `reserve_vault`.
     * Compute how many Commons tokens to mint based on current supply & curve formula.
     * Split inflow:

       * `reserve_share` → remains in `reserve_vault`
       * `common_pool_share` → move to `commons_treasury` via token transfer CPI.
       * Tributes: optionally to fee sink or protocol treasury.
     * Mint Commons tokens to user.

3. `sell_tokens`

   * Inputs: amount of Commons tokens to burn.
   * Steps:

     * Burn Commons tokens from user.
     * Compute payout in reserve using inverse of curve.
     * Apply exit tribute; transfer payout from `reserve_vault` → user.
     * Tribute share → `commons_treasury`.

4. `admin_update_params` (governance-gated)

   * Change kappa, friction, etc., only via DAO decisions.

**Notes:**

* Use **Anchor** for account serialization & CPI to SPL Token.
* For allowlist gating (trusted seed), integrate with `commons_hatch` or a separate membership token program (like CSTK equivalent).
* All heavy math is done in Rust with fixed-point decimals (e.g. 64.64 or 32.32); we can port formulas from existing ABC spec.([commons-stack.github.io][2])

---

### 3.2 Conviction Voting on Solana

**What it does on Ethereum**

Conviction Voting accumulates “conviction” for proposals as tokens remain staked; proposals pass once conviction crosses a threshold function relative to available funds and requested amount.([Commons Stack][1])

**Solana design**

**Accounts / PDAs:**

* `CVConfigPda`

  * Parameters: decay rate α, max ratio β, weight exponent, min threshold, etc.
  * Link to `commons_treasury`, `commons_token_mint`.
* `ProposalPda`

  * Fields:

    * creator, requested_amount, metadata_hash, status
    * `current_conviction`, `last_update_slot`
* `StakePda (user, proposal)`

  * `staked_amount`, `last_update_slot`.

**Time base:**

* Use `Clock` sysvar’s `slot` or `unix_timestamp` for approximate time deltas.

**Instructions:**

1. `create_proposal`

   * Create `ProposalPda`.
   * Attach IPFS/GitHub/Arweave hash for human-readable description.

2. `stake` / `unstake`

   * Transfers Commons tokens from user to a staking vault (per user or global).
   * On every stake/unstake:

     * Recompute user conviction and proposal conviction using exponential decay over elapsed time.
     * Update `StakePda` & `ProposalPda`.

3. `check_and_execute`

   * Can be triggered by anyone.
   * Recompute conviction since last update.
   * Compute threshold for requested funds based on CV function & available treasury.
   * If conviction ≥ threshold:

     * Mark proposal as `Approved`.
     * Transfer `requested_amount` from `commons_treasury` to recipient (or create a “funding escrow” account).
   * Otherwise just store updated conviction.

4. `withdraw_stake`

   * Let users exit their stake vault back into their wallet after unstaking.

**Integration:**

* Treasury account is either:

  * Owned by CV program itself (simple), or
  * A Realms/SPL Governance “governance account” that accepts CPI instructions from CV program as an authorized spender.
* CV parameters can be updated via DAO proposal (Realms) or via a “meta-proposal” inside CV itself.

---

### 3.3 Hatch on Solana

**What it does on Ethereum**

Hatch is the “two-phase launch”: a gated raise from a trusted seed, with min/max caps and param selection. If minimum isn’t met, funds are refunded; otherwise, the ABC is instantiated and tokens are minted and distributed.([Commons Stack][1])

**Solana design**

**Accounts / PDAs:**

* `HatchConfigPda`

  * Reserve asset mint, min_raise, max_raise, open/close slots.
  * Merkle root for trusted seed allowlist.
  * Pointer to final `CurveConfigPda` template & governance config.
* `ContributionPda (user)`

  * Tracks contributed amount, refunded flag.
* `HatchVault` – PDA token account holding contributions.

**Instructions:**

1. `initialize_hatch`

   * Set parameters, time window, Merkle root.

2. `contribute`

   * Verify user inclusion via Merkle proof, or via prior membership token.
   * Transfer reserve tokens from user → `HatchVault`.
   * Update `ContributionPda`.

3. `finalize_hatch`

   * After close_slot:

     * If total_contributed < min_raise → mark failed.

       * Allow `refund` calls.
     * Else:

       * Define final ABC parameters (can be pre-computed by Simulator).
       * Initialize `commons_token_mint`.
       * Initialize `commons_abc` with:

         * `reserve_vault` seeded from `HatchVault` per ABC design (some share to reserve, some to common pool).
       * Mint Commons tokens:

         * To contributors (pro-rata).
         * To a “reward pool” and other stakeholders.
       * Instantiate DAO (Realms) with Commons token as governance token.

4. `refund`

   * If hatch failed, let contributors withdraw their exact contribution.

5. `claim_tokens`

   * If hatch succeeded and vesting schedules apply, allow claiming over time.

**Notes:**

* You can keep Hatch and ABC as separate programs or have Hatch CPI into the ABC program once conditions are met.
* There’s already a conceptual spec from TEC’s Hatch process; you’re just porting it to Solana’s account model.([token-engineering-commons.gitbook.io][3])

---

### 3.4 Praise on Solana

**What it does**

Praise is basically an off-chain system where people “dish praise” (e.g. in Discord). A back-office process aggregates and scores contributions and then distributes rewards (tokens, reputation) based on those scores.([Commons Stack][1])

**Solana-flavored design**

**Off-chain:**

* Keep Praise mostly as-is:

  * Discord bot → database with `praises`, `contributors`, `scores`.
* Add **wallet binding**:

  * Each contributor connects a Solana wallet to their Praise profile and signs a message to prove ownership.

**On-chain program (`commons_rewards`):**

* `RewardEpochPda`

  * Epoch id, total_tokens, Merkle root of (wallet, amount) payouts.
* `claim_reward` instruction:

  * Verifies Merkle proof against root.
  * Transfers promised amount from a Reward Pool PDA to user’s wallet.

**Flow:**

1. Off-chain aggregator calculates scores for a period.
2. It computes reward allocation and Merkle root; posts:

   * On-chain: call `create_reward_epoch(root, total_tokens)`.
   * Off-chain: publish JSON with proofs.
3. Users call `claim_reward` with proof to get their tokens.

This keeps Praise logic off-chain but **trust-minimizes payout correctness** via Merkle proofs and an on-chain root.

---

### 3.5 Tokenlog on Solana

**What it does**

Tokenlog lets token holders prioritize GitHub issues using token-weighted voting. It’s mostly off-chain, reading token balances and writing results back to GitHub.([Commons Stack][1])

**Solana design**

This can also remain mostly off-chain, with minimal on-chain glue:

**Off-chain service:**

* Periodically fetch:

  * Commons token balances using an RPC endpoint or indexer.
  * GitHub issues in a given repo.
* Provide UI for token-weighted voting: users sign messages with Solana wallets (no transaction needed).

**Optional on-chain anchoring:**

* `TokenlogSnapshotPda`

  * Contains hash of scores for a given “snapshot id” (e.g. Git commit hash).
* Service submits final tally as `submit_snapshot(snapshot_id, scores_hash)`.
* DAO can require that certain parameter changes reference a specific `TokenlogSnapshotPda` (soft governance constraint).

This avoids a heavy on-chain implementation while keeping the trust story decent.

---

### 3.6 Quadratic Rank Choice Voting (QRCV) on Solana

QRCV is quadratic + ranked-choice voting; the Commons Stack site notes that details are still being fleshed out.([Commons Stack][1])

**Solana design (phase 2+):**

* `QRCVConfigPda`: max candidates, vote cost function, etc.
* `BallotPda (voter, election)`:

  * Stores ranked choices and total “voice credits” spent.
* `TallyPda (election)`:

  * Aggregate tallies per candidate.

On-chain tallying is doable but potentially expensive; an alternative:

* Store encrypted or blinded ballots on-chain.
* Off-chain tally service performs the heavy math and posts:

  * A proof-of-tally (e.g. zk or even simple multi-sig attestations).
  * A final result record to `TallyPda`.

Given complexity and limited spec, I’d treat QRCV as a **later-stage plugin** once core ABC + CV + Hatch are stable.

---

### 3.7 Commons Simulator integration

Commons Simulator (cadCAD) is chain-agnostic – it’s used to tune ABC & CV parameters before launch.([Commons Stack][1])

On Solana:

* Define a **parameter schema** in JSON (or TOML):

  * Bonding curve params.
  * CV params.
  * Hatch caps and timings.
* Build a CLI that:

  * Ingests this JSON.
  * Generates Anchor `initialize_*` transactions for `commons_hatch`, `commons_abc`, `commons_conviction_voting`.
* The simulator pipeline:

  1. Community uses Simulator (cadCAD) with candidate params.
  2. They converge on a configuration.
  3. Export config JSON → feed into CLI → actual Solana deployment.
  4. Optionally store the config hash on-chain in a `CommonsConfigPda`.

---

## 4. Phased implementation plan

### Phase 0 – Spec & scaffolding

1. **Confirm scope and dependencies**

   * Choose Solana stack:

     * Anchor for on-chain.
     * Realms (SPL Governance) or custom for DAO shell.
   * Choose reserve asset (likely USDC).

2. **Write tech specs**

   * ABC math & param constraints (port from existing ABC docs).([commons-stack.github.io][2])
   * CV formula & thresholds (draw from CV papers + 1Hive implementation).([Giveth][4])
   * Hatch process, including trusted seed gating & vesting windows.

3. **Repo layout**

   * Monorepo:

     * `/programs/commons_hatch`
     * `/programs/commons_abc`
     * `/programs/commons_conviction_voting`
     * `/programs/commons_rewards`
     * `/offchain/praise-service`
     * `/offchain/tokenlog-service`
     * `/offchain/simulator-pipeline`

### Phase 1 – Hatch + ABC

1. **Implement `commons_hatch`**

   * All instructions described above; use integration tests that simulate success/fail hatches.

2. **Implement `commons_abc`**

   * Start with a simpler power curve; address numeric stability.
   * Integrate with hatch: `finalize_hatch` calls into `initialize_curve`.

3. **Front-end / CLI**

   * Minimal React/Next.js or CLI to:

     * Display hatch status.
     * Allow contributions via wallet.
     * Show bonding curve price chart.

4. **Audit & simulation**

   * Use the Commons Simulator (or a lighter Rust sim) to generate test params and run multi-scenario tests.

### Phase 2 – Conviction Voting

1. **Implement `commons_conviction_voting`**

   * Core stake/unstake, conviction update, threshold check.
   * Use a simplified threshold function first; refine later.

2. **Treasury wiring**

   * Decide whether CV owns the treasury account or issues CPI into a Realms-managed treasury.

3. **Front-end**

   * Proposals list, conviction over time, stake flow.
   * Visuals similar to 1Hive’s CV app for familiarity.([GitHub][5])

4. **Integration tests**

   * ABC → CV flow:

     * Bonding curve inflows accumulate in `commons_treasury`.
     * CV proposals draw down from that treasury when approved.

### Phase 3 – Praise & rewards

1. **Praise service**

   * Fork or interoperate with existing Praise system where possible.([GitHub][6])
   * Add Solana wallet binding & reward export.

2. **Implement `commons_rewards`**

   * Merkle-based distribution as described.

3. **Governance practice**

   * Decide whether Praise distributions require DAO sign-off or are automatic once epoch root is posted.

### Phase 4 – Tokenlog-style integration

1. **Tokenlog service**

   * Off-chain tool that:

     * Reads Commons token balances from Solana RPC.
     * Offers token-weighted voting on GitHub issues.
   * Optionally posts results back to GitHub via a bot.

2. **On-chain anchor (optional)**

   * Add `TokenlogSnapshotPda` and a way to reference snapshot IDs in DAO proposals (e.g. “adopt parameters from snapshot X”).

### Phase 5 – QRCV + advanced governance

1. **Design QRCV spec concretely**

   * How many choices, how quadratic weighting is applied, what the tally returns (one winner, ranking, etc.).

2. **Prototype**

   * Start with off-chain tallying; store only commitments on-chain.

3. **Integrate**

   * Use QRCV for:

     * Parameter selection rounds.
     * Large “governance mode” decisions (e.g. change CV parameters, adjust ABC friction).

---

## 5. Migration / interoperability considerations

If you care about **interoperability with existing EVM Commons/TE Commons** ecosystems:

* Use stablecoin bridges (e.g. USDC) as the shared reserve asset.
* Represent membership (trusted seed) via:

  * Airdropped SPL tokens on Solana to addresses corresponding to Ethereum wallets that hold CSTK or other gating tokens.
  * Off-chain attestations + Merkle roots for allowlists.

You can also expose an **indexer API** that mirrors the same data shapes as current Aragon-based deployments, making cross-chain dashboards easier to build.

---

[1]: https://www.commonsstack.org/solutions "Commons Stack - Our Solutions"
[2]: https://commons-stack.github.io/augmented-tbc-design/?utm_source=chatgpt.com "Augmented Bonding Curve Design - Commons Stack"
[3]: https://token-engineering-commons.gitbook.io/tec-handbook/hatch-101/welcome-hatchers/the-hatch-process?utm_source=chatgpt.com "Hatch Process | TEC Handbook - TEC Source - GitBook"
[4]: https://blog.giveth.io/conviction-voting-a-novel-continuous-decision-making-alternative-to-governance-aa746cfb9475?utm_source=chatgpt.com "Conviction Voting: A Novel Continuous Decision Making ..."
[5]: https://github.com/1Hive/conviction-voting-app?utm_source=chatgpt.com "1Hive/conviction-voting-app"
[6]: https://github.com/givepraise?utm_source=chatgpt.com "Praise"

