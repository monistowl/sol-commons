import { expect } from "chai";
import * as crypto from "crypto";
import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Token, TOKEN_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { loadOffchainPayload } from "./helpers/offchainPayload";
import { CommonsHatch } from "../target/types/commons_hatch";
import { CommonsAbc } from "../target/types/commons_abc";
import { CommonsConvictionVoting } from "../target/types/commons_conviction_voting";
import { CommonsRewards } from "../target/types/commons_rewards";

const DECIMALS = 6;
const FRICTION = 50_000;
const KAPPA = 2;
const EXPONENT = 1;
const INITIAL_PRICE = 1;
const DECAY_RATE = 500_000;
const MAX_RATIO = 1_000_000;
const WEIGHT_EXPONENT = 1_000_000;
const MIN_THRESHOLD = 100_000;

anchor.setProvider(anchor.AnchorProvider.env());
const provider = anchor.getProvider() as anchor.AnchorProvider;
const hatchProgram = anchor.workspace.commonsHatch as Program<CommonsHatch>;
const abcProgram = anchor.workspace.commonsAbc as Program<CommonsAbc>;
const cvProgram = anchor.workspace.commonsConvictionVoting as Program<CommonsConvictionVoting>;
const rewardsProgram = anchor.workspace.commonsRewards as Program<CommonsRewards>;

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForSlot(connection: anchor.web3.Connection, targetSlot: number) {
  while ((await connection.getSlot()) < targetSlot) {
    await sleep(50);
  }
}

function hashContributorAllocation(
  contributor: anchor.web3.PublicKey,
  allocation: number
): Buffer {
  const allocationBuffer = Buffer.alloc(8);
  allocationBuffer.writeBigUInt64LE(BigInt(allocation));
  return crypto
    .createHash("sha256")
    .update(Buffer.concat([contributor.toBuffer(), allocationBuffer]))
    .digest();
}

function toU64Buffer(value: number): Buffer {
  const buffer = Buffer.alloc(8);
  buffer.writeBigUInt64LE(BigInt(value));
  return buffer;
}

describe("full lifecycle", () => {
  it("runs hatch → ABC → conviction voting → rewards", async () => {
    const reserveTokenMint = await Token.createMint(
      provider.connection,
      provider.wallet.payer,
      provider.wallet.publicKey,
      null,
      DECIMALS,
      TOKEN_PROGRAM_ID
    );

    const reserveToken = new Token(
      provider.connection,
      reserveTokenMint.publicKey,
      TOKEN_PROGRAM_ID,
      provider.wallet.payer
    );

    const depositAmount = 1_000_000; // 1 token unit with 6 decimals
    const userReserveAccount = await reserveToken.getOrCreateAssociatedAccountInfo(
      provider.wallet.publicKey
    );
    await reserveToken.mintTo(
      userReserveAccount.address,
      provider.wallet.publicKey,
      [],
      depositAmount
    );

    const currentSlot = await provider.connection.getSlot();
    const openSlot = Math.max(0, currentSlot - 1);
    const closeSlot = currentSlot + 2;
    const merkleRoot = hashContributorAllocation(provider.wallet.publicKey, depositAmount);
    const [hatchConfig] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("hatch_config"), reserveTokenMint.publicKey.toBuffer()],
      hatchProgram.programId
    );
    const [hatchVault] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("hatch_vault"), reserveTokenMint.publicKey.toBuffer()],
      hatchProgram.programId
    );

    await hatchProgram.methods
      .initializeHatch(
        new anchor.BN(depositAmount),
        new anchor.BN(depositAmount),
        new anchor.BN(openSlot),
        new anchor.BN(closeSlot),
        Array.from(merkleRoot)
      )
      .accounts({
        hatchConfig,
        reserveAssetMint: reserveTokenMint.publicKey,
        hatchVault,
        authority: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    const [contribution] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("contribution"), provider.wallet.publicKey.toBuffer()],
      hatchProgram.programId
    );

    await hatchProgram.methods
      .contribute(new anchor.BN(depositAmount), new anchor.BN(depositAmount), [])
      .accounts({
        hatchConfig,
        contribution,
        hatchVault,
        userReserveTokenAccount: userReserveAccount.address,
        authority: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    await waitForSlot(provider.connection, closeSlot);

    const reserveVault = anchor.web3.Keypair.generate();
    const commonsTreasury = anchor.web3.Keypair.generate();

    const [commonsTokenMint] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("commons_token_mint"), hatchConfig.toBuffer()],
      hatchProgram.programId
    );
    const [curveConfig] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("curve_config"), commonsTokenMint.toBuffer()],
      abcProgram.programId
    );

    await hatchProgram.methods
      .finalizeHatch(
        new anchor.BN(KAPPA),
        new anchor.BN(EXPONENT),
        new anchor.BN(INITIAL_PRICE),
        new anchor.BN(FRICTION)
      )
      .accounts({
        hatchConfig,
        reserveAssetMint: reserveTokenMint.publicKey,
        authority: provider.wallet.publicKey,
        curveConfig,
        commonsTokenMint,
        reserveVault: reserveVault.publicKey,
        commonsTreasury: commonsTreasury.publicKey,
        commonsAbcProgram: abcProgram.programId,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([reserveVault, commonsTreasury])
      .rpc();

    const commonsToken = new Token(
      provider.connection,
      commonsTokenMint,
      TOKEN_PROGRAM_ID,
      provider.wallet.payer
    );

    const userCommonsAccount = await commonsToken.getOrCreateAssociatedAccountInfo(
      provider.wallet.publicKey
    );

    await hatchProgram.methods
      .claim()
      .accounts({
        hatchConfig,
        contribution,
        commonsTokenMint,
        curveConfig,
        userCommonsTokenAccount: userCommonsAccount.address,
        authority: provider.wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    const userCommonsBalance = (await commonsToken.getAccountInfo(userCommonsAccount.address)).amount.toNumber();
    expect(userCommonsBalance).to.equal(depositAmount);

    const rewardFund = anchor.web3.Keypair.generate();
    const rewardPoolAmount = depositAmount - 200_000;
    const stakeAmount = 200_000;

    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(rewardFund.publicKey, anchor.web3.LAMPORTS_PER_SOL)
    );

    const rewardFundAccount = await commonsToken.getOrCreateAssociatedAccountInfo(
      rewardFund.publicKey
    );

    await commonsToken.transfer(
      userCommonsAccount.address,
      rewardFundAccount.address,
      provider.wallet.publicKey,
      [],
      new anchor.BN(rewardPoolAmount)
    );

    const [cvConfig] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("cv_config")],
      cvProgram.programId
    );
    const [stakingVault] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("staking_vault"), cvConfig.toBuffer()],
      cvProgram.programId
    );

    await cvProgram.methods
      .initializeCvConfig(
        new anchor.BN(DECAY_RATE),
        new anchor.BN(MAX_RATIO),
        new anchor.BN(WEIGHT_EXPONENT),
        new anchor.BN(MIN_THRESHOLD)
      )
      .accounts({
        cvConfig,
        commonsTreasury: commonsTreasury.publicKey,
        commonsTokenMint,
        stakingVault,
        authority: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    const requestedAmount = 1_000;
    const requestBuffer = toU64Buffer(requestedAmount);
    const [proposal] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("proposal"), provider.wallet.publicKey.toBuffer(), requestBuffer],
      cvProgram.programId
    );

    await cvProgram.methods
      .createProposal(new anchor.BN(requestedAmount), "full-lifecycle")
      .accounts({
        proposal,
        authority: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
      })
      .rpc();

    const [stakeAccount] = await anchor.web3.PublicKey.findProgramAddress(
      [
        Buffer.from("stake"),
        provider.wallet.publicKey.toBuffer(),
        proposal.toBuffer(),
      ],
      cvProgram.programId
    );

    await cvProgram.methods
      .stakeTokens(new anchor.BN(stakeAmount))
      .accounts({
        stakeAccount,
        cvConfig,
        proposal,
        commonsTokenMint,
        userCommonsTokenAccount: userCommonsAccount.address,
        stakingVault,
        authority: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
      })
      .rpc();

    const recipient = anchor.web3.Keypair.generate();
    const recipientReserveAccount = await reserveToken.getOrCreateAssociatedAccountInfo(
      recipient.publicKey
    );

    const treasuryBefore = (
      await reserveToken.getAccountInfo(commonsTreasury.publicKey)
    ).amount.toNumber();

    await cvProgram.methods
      .checkAndExecute()
      .accounts({
        cvConfig,
        proposal,
        commonsTreasury: commonsTreasury.publicKey,
        recipientTokenAccount: recipientReserveAccount.address,
        authority: provider.wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
      })
      .rpc();

    const treasuryAfter = (
      await reserveToken.getAccountInfo(commonsTreasury.publicKey)
    ).amount.toNumber();
    const recipientAfter = (
      await reserveToken.getAccountInfo(recipientReserveAccount.address)
    ).amount.toNumber();

    expect(treasuryBefore - treasuryAfter).to.equal(requestedAmount);
    expect(recipientAfter).to.equal(requestedAmount);

    const payload = await loadOffchainPayload({
      praiseEvents: [
        {
          address: rewardFund.publicKey.toBase58(),
          amount: 1234,
          event: "reward-cycle",
        },
      ],
    });

    const epochId = 1;
    const epochBuffer = Buffer.alloc(8);
    epochBuffer.writeBigUInt64LE(BigInt(epochId));
    const [rewardEpoch] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("reward_epoch"), epochBuffer],
      rewardsProgram.programId
    );
    const [rewardVaultAuthority] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("reward_vault_authority"), epochBuffer],
      rewardsProgram.programId
    );
    const [rewardVault] = await anchor.web3.PublicKey.findProgramAddress(
      [Buffer.from("reward_vault"), epochBuffer],
      rewardsProgram.programId
    );

    await rewardsProgram.methods
      .createRewardEpoch(
        new anchor.BN(epochId),
        new anchor.BN(payload.batch.totalTokens),
        Array.from(Buffer.from(payload.batch.merkleRoot, "hex"))
      )
      .accounts({
        rewardEpoch,
        rewardVault,
        rewardVaultAuthority,
        authority: provider.wallet.publicKey,
        rewardMint: commonsTokenMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    await commonsToken.transfer(
      rewardFundAccount.address,
      rewardVault,
      rewardFund,
      [],
      new anchor.BN(payload.batch.totalTokens)
    );

    const beforeClaimBalance = (
      await commonsToken.getAccountInfo(rewardFundAccount.address)
    ).amount.toNumber();

    const claimProof = payload.proofs[rewardFund.publicKey.toBase58()];
    expect(claimProof).to.be.ok;

    const [claimStatus] = await anchor.web3.PublicKey.findProgramAddress(
      [
        Buffer.from("reward_claim"),
        epochBuffer,
        rewardFund.publicKey.toBuffer(),
      ],
      rewardsProgram.programId
    );

    await rewardsProgram.methods
      .claimReward(
        new anchor.BN(epochId),
        new anchor.BN(claimProof.amount),
        claimProof.proof.map((node) => Array.from(node))
      )
      .accounts({
        rewardEpoch,
        rewardVault,
        rewardVaultAuthority,
        userRewardTokenAccount: rewardFundAccount.address,
        claimStatus,
        authority: provider.wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    const afterClaimBalance = (
      await commonsToken.getAccountInfo(rewardFundAccount.address)
    ).amount.toNumber();
    expect(afterClaimBalance - beforeClaimBalance).to.equal(claimProof.amount);
  });
});
