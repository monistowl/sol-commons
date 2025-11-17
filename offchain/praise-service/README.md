# Praise Service (scaffold)

This scaffold represents the off-chain Praise leaderboard service. The production service would collect community praise events, score them, and then post the resulting reward batch to `commons_rewards` as a Merkle root.

## What’s here
- `index.js`: praise collector that aggregates points per address, builds a Merkle batch, and exposes helpers for proofs + epoch snapshots.
- `config.json`: declares RPC hooks, base reward size, and a default set of claims so the service can bootstrap even when no events are available.
- `README` note linking to the planned integration tests.
