# Commons Conviction Voting

This program is responsible for the Conviction Voting module of the Commons Stack on Solana.

## Solana Design

### Accounts / PDAs:

*   `CVConfigPda`
    *   Parameters: decay rate α, max ratio β, weight exponent, min threshold, etc.
    *   Link to `commons_treasury`, `commons_token_mint`.
*   `ProposalPda`
    *   Fields:
        *   creator, requested\_amount, metadata\_hash, status
        *   `current_conviction`, `last_update_slot`
*   `StakePda (user, proposal)`
    *   `staked_amount`, `last_update_slot`.

### Time base:

*   Use `Clock` sysvar’s `slot` or `unix_timestamp` for approximate time deltas.

### Instructions:

1.  `create_proposal`
    *   Create `ProposalPda`.
    *   Attach IPFS/GitHub/Arweave hash for human-readable description.
2.  `stake` / `unstake`
    *   Transfers Commons tokens from user to a staking vault (per user or global).
    *   On every stake/unstake:
        *   Recompute user conviction and proposal conviction using exponential decay over elapsed time.
        *   Update `StakePda` & `ProposalPda`.
3.  `check_and_execute`
    *   Can be triggered by anyone.
    *   Recompute conviction since last update.
    *   Compute threshold for requested funds based on CV function & available treasury.
    *   If conviction ≥ threshold:
        *   Mark proposal as `Approved`.
        *   Transfer `requested_amount` from `commons_treasury` to recipient (or create a “funding escrow” account).
    *   Otherwise just store updated conviction.
4.  `withdraw_stake`
    *   Let users exit their stake vault back into their wallet after unstaking.

### Integration:

*   Treasury account is either:
    *   Owned by CV program itself (simple), or
    *   A Realms/SPL Governance “governance account” that accepts CPI instructions from CV program as an authorized spender.
*   CV parameters can be updated via DAO proposal (Realms) or via a “meta-proposal” inside CV itself.