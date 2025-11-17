# Simulator Pipeline (scaffold)

This service will eventually run cadCAD or similar simulations to suggest governance parameters. It will export config snapshots consumed by the on-chain programs.

- `index.js`: exports a deterministic `runSimulation` that mixes config metadata with reproducible randomness to emit updated governance parameters and metrics whenever the integration test runs.
- `config.json`: defines scenario metadata plus base parameter seeds that the simulator blends with randomness.

## Pipeline integration

`offchain/pipeline/index.js` runs this simulator to surface deterministic parameters (`fundingRatio`, `convictionDecay`, `rewardMultiplier`) plus scenario metadata that can flow into the `commons_abc` and `commons_conviction_voting` initialization instructions. The documented payload makes it easy to replay the same seeds in CI or on a devnet validator.
