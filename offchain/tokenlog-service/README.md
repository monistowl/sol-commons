# Tokenlog Service (scaffold)

This service now shows how a simple Tokenlog-inspired workflow can fetch live GitHub issues plus on-chain balances before summarizing the priorities for the DAO.

## Structure
- `index.js`: exposes `fetchGithubIssues` (falls back to mocks when GitHub is unreachable) and `sampleBalances` (tagged with a snapshot timestamp) so tests can exercise both the GitHub pull and the Commons-weighted balances.
- `config.json`: tracks the repo to query, fallback mock data, and a per-call issue limit so the service can be tuned before productionizing.
