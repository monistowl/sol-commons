# Commons Rewards

This program is responsible for the Praise rewards distribution of the Commons Stack on Solana.

## Solana-flavored design

### Off-chain:

*   Keep Praise mostly as-is:
    *   Discord bot → database with `praises`, `contributors`, `scores`.
*   Add **wallet binding**:
    *   Each contributor connects a Solana wallet to their Praise profile and signs a message to prove ownership.

### On-chain program (`commons_rewards`):

*   `RewardEpochPda`
    *   Epoch id, total\_tokens, Merkle root of (wallet, amount) payouts.
*   `claim_reward` instruction:
    *   Verifies Merkle proof against root.
    *   Transfers promised amount from a Reward Pool PDA to user’s wallet.

### Flow:

1.  Off-chain aggregator calculates scores for a period.
2.  It computes reward allocation and Merkle root; posts:
    *   On-chain: call `create_reward_epoch(root, total_tokens)`.
    *   Off-chain: publish JSON with proofs.
3.  Users call `claim_reward` with proof to get their tokens.

This keeps Praise logic off-chain but **trust-minimizes payout correctness** via Merkle proofs and an on-chain root.