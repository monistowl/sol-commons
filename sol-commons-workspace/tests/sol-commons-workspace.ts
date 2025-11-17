import { expect } from "chai";
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolCommonsWorkspace } from "../target/types/sol_commons_workspace";
import { CommonsRewards } from "../target/types/commons_rewards";
import { Token, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { loadOffchainPayload } from "./helpers/offchainPayload";

describe("sol-commons-workspace", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.solCommonsWorkspace as Program<SolCommonsWorkspace>;

  it("Is initialized!", async () => {
    const tx = await program.methods.initialize().rpc();
    console.log("Your transaction signature", tx);
  });

  it("bridges offchain praise + sim data into commons_rewards", async () => {
    const provider = anchor.getProvider() as anchor.AnchorProvider;
    const rewards = anchor.workspace.commonsRewards as Program<CommonsRewards>;
    const payload = await loadOffchainPayload({
      praiseEvents: [
        {
          address: provider.wallet.publicKey.toBase58(),
          amount: 250,
          event: "integration-claim",
        },
      ],
    });
    const { batch, issues, balances, simulation, proofs } = payload;
    expect(issues).to.be.an("array").with.length.greaterThan(0);
    expect(balances).to.be.an("array");
    expect(simulation.params).to.include.keys(["fundingRatio", "convictionDecay"]);
    expect(simulation.metrics).to.include.keys(["confidence", "projectedPayout"]);
    const claimProof = proofs[provider.wallet.publicKey.toBase58()];
    expect(claimProof).to.be.ok;

    const epochId = 1;
    const epochBuffer = Buffer.alloc(8);
    epochBuffer.writeBigUInt64LE(BigInt(epochId));
    const [rewardEpochPda] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("reward_epoch"), epochBuffer],
      rewards.programId
    );
    const [rewardVaultAuthority] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("reward_vault_authority"), epochBuffer],
      rewards.programId
    );
    const [rewardVault] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("reward_vault"), epochBuffer],
      rewards.programId
    );

    const mint = await Token.createMint(
      provider.connection,
      provider.wallet.payer,
      provider.wallet.publicKey,
      null,
      6,
      TOKEN_PROGRAM_ID
    );

    await rewards.methods
      .createRewardEpoch(new anchor.BN(epochId), new anchor.BN(batch.totalTokens), Buffer.from(batch.merkleRoot, "hex"))
      .accounts({
        rewardEpoch: rewardEpochPda,
        rewardVault,
        rewardVaultAuthority,
        rewardMint: mint.publicKey,
        authority: provider.wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    await mint.mintTo(rewardVault, provider.wallet.publicKey, [], batch.totalTokens);

    const stored = await rewards.account.rewardEpoch.fetch(rewardEpochPda);
    if (stored.totalTokens.toNumber() !== batch.totalTokens) {
      throw new Error("batch not stored");
    }

    const userRewardAccount = await mint.getOrCreateAssociatedAccountInfo(provider.wallet.publicKey);
    const [claimStatusPda] = await anchor.web3.PublicKey.findProgramAddress(
      [
        Buffer.from("reward_claim"),
        epochBuffer,
        provider.wallet.publicKey.toBuffer(),
      ],
      rewards.programId
    );

    await rewards.methods
      .claimReward(
        new anchor.BN(epochId),
        new anchor.BN(claimProof.amount),
        claimProof.proof.map((node) => Array.from(node))
      )
      .accounts({
        rewardEpoch: rewardEpochPda,
        rewardVault,
        rewardVaultAuthority,
        userRewardTokenAccount: userRewardAccount.address,
        claimStatus: claimStatusPda,
        authority: provider.wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    const updatedAccount = await mint.getAccountInfo(userRewardAccount.address);
    expect(updatedAccount.amount.toNumber()).to.equal(claimProof.amount);
  });
});
