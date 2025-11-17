import { expect } from "chai";
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolCommonsWorkspace } from "../target/types/sol_commons_workspace";
import { CommonsRewards } from "../target/types/commons_rewards";
import { Token, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { startPraiseService } from "../../offchain/praise-service/index";
import { fetchGithubIssues, sampleBalances } from "../../offchain/tokenlog-service/index";
import { runSimulation } from "../../offchain/simulator-pipeline/index";

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
    const praiseService = startPraiseService();
    const batch = praiseService.generateRewardBatch();
    const issues = await fetchGithubIssues();
    const balances = sampleBalances();
    expect(issues).to.be.an("array").with.length.greaterThan(0);
    expect(balances).to.be.an("array");
    runSimulation();

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

    const mint = await Token.createMint(
      provider.connection,
      provider.wallet.payer,
      provider.wallet.publicKey,
      null,
      6,
      TOKEN_PROGRAM_ID
    );
    const rewardVault = await mint.createAccount(rewardVaultAuthority);
    await mint.mintTo(rewardVault, provider.wallet.publicKey, [], batch.totalTokens);

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

    const stored = await rewards.account.rewardEpoch.fetch(rewardEpochPda);
    if (stored.totalTokens.toNumber() !== batch.totalTokens) {
      throw new Error("batch not stored");
    }
  });
});
