# Praise Service (scaffold)

This scaffold represents the off-chain Praise leaderboard service. The production service would collect community praise events, score them, and then post the resulting reward batch to `commons_rewards` as a Merkle root.

## What’s here
- `index.js`: praise collector that aggregates points per address, builds a Merkle batch, and exposes helpers for proofs + epoch snapshots.
- `config.json`: declares RPC hooks, base reward size, and a default set of claims so the service can bootstrap even when no events are available.
- `README` note linking to the planned integration tests.

## Pipeline integration

`offchain/pipeline/index.js` uses this service to build the Merkle batch consumed by `commons_rewards.create_reward_epoch`. The pipeline optionally injects praise events, then stores the epoch root and proof map for TypeScript/Mocha clients. Each claim path on-chain uses the `[b"reward_claim", epoch_id, claimer_pubkey]` PDA and the `ClaimStatus` account to enforce replay protection, so the JSON emitted here can be trusted to match the PDA derivations documented in the main README.
