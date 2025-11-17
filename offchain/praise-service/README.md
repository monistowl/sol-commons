# Praise Service (scaffold)

This scaffold represents the off-chain Praise leaderboard service. The production service would collect community praise events, score them, and then post the resulting reward batch to `commons_rewards` as a Merkle root.

## What’s here
- `index.js`: simple scorer that emits placeholder data and exposes a `generateRewardBatch` helper.
- Config hooks in `config.json` to point at RPC endpoints/providers.
- A `README` note linking to the planned integration tests.
