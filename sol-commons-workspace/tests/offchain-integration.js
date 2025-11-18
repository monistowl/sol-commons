const anchor = require('@coral-xyz/anchor');
const {
  createInitializeMint2Instruction,
  getAccount,
  getMinimumBalanceForRentExemptMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  MINT_SIZE,
  TOKEN_PROGRAM_ID,
} = require('@solana/spl-token');
const { expect } = require('chai');
const { loadOffchainPayload } = require('./helpers/offchainPayload');
const rewardIdl = require('../../offchain/commons_rewards.idl.json');

const validatorSecret = process.env.OFFCHAIN_VALIDATOR_SECRET;
function buildValidatorWallet() {
  if (!validatorSecret) {
    return anchor.web3.Keypair.generate();
  }
  const secretKey = new Uint8Array(JSON.parse(validatorSecret));
  return anchor.web3.Keypair.fromSecretKey(secretKey);
}

describe('offchain scaffolding integration', function () {
  this.timeout(10000);

  it('generates reward batch and ties helpers together', async function () {
    const payload = await loadOffchainPayload({
      praiseEvents: [{ event: 'test-event', amount: 42 }],
    });
    const { batch, issues, balances, simulation } = payload;
    expect(batch).to.include.all.keys(['merkleRoot', 'totalTokens', 'snapshotDate']);
    expect(batch.claims).to.be.an('array').with.length.greaterThan(0);

    expect(issues).to.be.an('array').with.length.greaterThan(0);
    expect(balances).to.be.an('array');

    expect(simulation.params).to.include.keys(['fundingRatio', 'convictionDecay']);
    expect(simulation.metrics).to.include.keys(['confidence', 'projectedPayout']);
  });

    it('produces reward instructions from Praise outputs', async function () {
      const walletKeypair = buildValidatorWallet();
      const payload = await loadOffchainPayload({
        praiseEvents: [
          {
            address: walletKeypair.publicKey.toBase58(),
            amount: 1234,
            event: 'claimer-event',
          },
        ],
      });
      const batch = payload.batch;
    const epochId = 1;
    const rootBuffer = Buffer.from(batch.merkleRoot, 'hex');

    const provider = new anchor.AnchorProvider(
      new anchor.web3.Connection('http://127.0.0.1:8899', 'confirmed'),
      new anchor.Wallet(walletKeypair),
      anchor.AnchorProvider.defaultOptions()
    );
    anchor.setProvider(provider);
    const payer = walletKeypair;

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
    const [rewardVault] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from('reward_vault'), epochBuffer],
      programId
    );

    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(payer.publicKey, anchor.web3.LAMPORTS_PER_SOL),
      'confirmed'
    );
    const mintKeypair = anchor.web3.Keypair.generate();
    const lamports = await getMinimumBalanceForRentExemptMint(provider.connection);
    const createMintTx = new anchor.web3.Transaction().add(
      anchor.web3.SystemProgram.createAccount({
        fromPubkey: payer.publicKey,
        newAccountPubkey: mintKeypair.publicKey,
        space: MINT_SIZE,
        lamports,
        programId: TOKEN_PROGRAM_ID,
      }),
      createInitializeMint2Instruction(
        mintKeypair.publicKey,
        6,
        provider.wallet.publicKey,
        null,
        TOKEN_PROGRAM_ID
      )
    );
    await anchor.web3.sendAndConfirmTransaction(provider.connection, createMintTx, [
      payer,
      mintKeypair,
    ]);
    const mint = mintKeypair.publicKey;

    await rewards.methods
      .createRewardEpoch(new anchor.BN(epochId), new anchor.BN(batch.totalTokens), Array.from(rootBuffer))
      .accounts({
        rewardEpoch: rewardEpochPda,
        rewardVault,
        rewardVaultAuthority,
        authority: provider.wallet.publicKey,
        rewardMint: mint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    await mintTo(
      provider.connection,
      payer,
      mint,
      rewardVault,
      walletKeypair,
      batch.totalTokens
    );

    const claimProof = payload.proofs[walletKeypair.publicKey.toBase58()];
    expect(claimProof).to.be.ok;

    const providerRewardAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      payer,
      mint,
      provider.wallet.publicKey
    );
    const [claimStatus] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from('reward_claim'), epochBuffer, walletKeypair.publicKey.toBuffer()],
      programId
    );

    const vaultBefore = await getAccount(provider.connection, rewardVault);

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
        userRewardTokenAccount: providerRewardAccount.address,
        claimStatus,
        authority: provider.wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    const vaultAfter = await getAccount(provider.connection, rewardVault);
    const userAfter = await getAccount(provider.connection, providerRewardAccount.address);
    expect(Number(vaultBefore.amount) - Number(vaultAfter.amount)).to.equal(claimProof.amount);
    expect(Number(userAfter.amount)).to.equal(claimProof.amount);
  });
});
