# Simulator Pipeline (scaffold)

This service will eventually run cadCAD or similar simulations to suggest governance parameters. It will export config snapshots consumed by the on-chain programs.

- `index.js`: exports a deterministic `runSimulation` that mixes config metadata with reproducible randomness to emit updated governance parameters and metrics whenever the integration test runs.
- `config.json`: defines scenario metadata plus base parameter seeds that the simulator blends with randomness.
