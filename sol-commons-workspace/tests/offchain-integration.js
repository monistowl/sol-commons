const anchor = require('@coral-xyz/anchor');
const { TOKEN_PROGRAM_ID } = require('@solana/spl-token');
const { expect } = require('chai');
const { startPraiseService } = require('../../offchain/praise-service/index');
const { fetchGithubIssues, sampleBalances } = require('../../offchain/tokenlog-service/index');
const { runSimulation } = require('../../offchain/simulator-pipeline/index');
const rewardIdl = require('../../offchain/commons_rewards.idl.json');

describe('offchain scaffolding integration', () => {
  it('generates reward batch and ties helpers together', async () => {
    const praiseService = startPraiseService();
    praiseService.collect('test-event');
    const batch = praiseService.generateRewardBatch();
    expect(batch).to.include.all.keys(['merkleRoot', 'totalTokens', 'snapshotDate']);
    expect(batch.claims).to.be.an('array').with.length.greaterThan(0);

    const issues = await fetchGithubIssues();
    const balances = sampleBalances();
    expect(issues).to.be.an('array').with.length.greaterThan(0);
    expect(balances).to.be.an('array');

    const sim = runSimulation();
    expect(sim.params).to.include.keys(['fundingRatio', 'convictionDecay']);
    expect(sim.metrics).to.include.keys(['confidence', 'projectedPayout']);
  });

  it('produces reward instructions from Praise outputs', async () => {
    const praiseService = startPraiseService();
    const batch = praiseService.generateRewardBatch();
    const epochId = 1;
    const rootBuffer = Buffer.from(batch.merkleRoot, 'hex');

    const walletKeypair = anchor.web3.Keypair.generate();
    const provider = new anchor.AnchorProvider(
      new anchor.web3.Connection('http://127.0.0.1:8899', 'confirmed'),
      new anchor.Wallet(walletKeypair),
      anchor.AnchorProvider.defaultOptions()
    );
    anchor.setProvider(provider);

    const rewards = new anchor.Program(rewardIdl, provider);
    const programId = rewards.programId;

    const epochBuffer = Buffer.alloc(8);
    epochBuffer.writeBigUInt64LE(BigInt(epochId));
    const [rewardEpochPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from('reward_epoch'), epochBuffer],
      programId
    );
    const [rewardVaultAuthority] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from('reward_vault_authority'), epochBuffer],
      programId
    );

    const rewardVault = anchor.web3.Keypair.generate().publicKey;
    const rewardMint = anchor.web3.Keypair.generate().publicKey;
    const createIx = await rewards.instruction.createRewardEpoch(
      new anchor.BN(epochId),
      new anchor.BN(batch.totalTokens),
      Array.from(rootBuffer),
      {
        accounts: {
          rewardEpoch: rewardEpochPda,
          rewardVault,
          rewardVaultAuthority,
          authority: provider.wallet.publicKey,
          rewardMint,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: anchor.web3.SystemProgram.programId,
        },
      }
    );
    expect(createIx.programId.equals(programId)).to.be.true;
    expect(createIx.keys.map((k) => k.pubkey.toBase58())).to.include(rewardEpochPda.toBase58());

    const firstClaim = batch.claims[0];
    const proof = praiseService.proofFor(firstClaim.address);
    const userRewardTokenAccount = anchor.web3.Keypair.generate().publicKey;
    const claimIx = await rewards.instruction.claimReward(
      new anchor.BN(epochId),
      new anchor.BN(firstClaim.amount),
      proof.proof.map((node) => Array.from(node)),
      {
        accounts: {
          rewardEpoch: rewardEpochPda,
          rewardVault,
          rewardVaultAuthority,
          userRewardTokenAccount,
          authority: provider.wallet.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: anchor.web3.SystemProgram.programId,
        },
      }
    );
    expect(claimIx.keys.map((k) => k.pubkey.toBase58())).to.include(
      userRewardTokenAccount.toBase58()
    );
  });
});
