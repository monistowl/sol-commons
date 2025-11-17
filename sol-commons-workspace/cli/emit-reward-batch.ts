import { assembleOffchainPayload } from "../../offchain/pipeline/index";
import { PublicKey } from "@solana/web3.js";
import { readFileSync, writeFileSync } from "fs";

const PROGRAM_ID = new PublicKey("GccA6L8BUnkZVeUAdSAeoiFFCVynf6GZbBTPZfCj7tpY");

interface CliArgs {
  epoch?: number;
  output?: string;
  praiseEventsFile?: string;
}

function parseArgs(argv: string[]): CliArgs {
  const args: CliArgs = {};
  for (let i = 0; i < argv.length; i += 1) {
    const current = argv[i];
    if (current === "--epoch" && i + 1 < argv.length) {
      args.epoch = Number(argv[++i]);
    } else if (current === "--output" && i + 1 < argv.length) {
      args.output = argv[++i];
    } else if (current === "--praise-events-file" && i + 1 < argv.length) {
      args.praiseEventsFile = argv[++i];
    }
  }
  return args;
}

function claimStatusPda(epochBuffer: Buffer, claimer: PublicKey) {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("reward_claim"), epochBuffer, claimer.toBuffer()],
    PROGRAM_ID
  );
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const epochId = args.epoch ?? 1;
  const epochBuffer = Buffer.alloc(8);
  epochBuffer.writeBigUInt64LE(BigInt(epochId));

  const payloadOptions = {} as { praiseEvents?: unknown[] };
  if (args.praiseEventsFile) {
    payloadOptions.praiseEvents = JSON.parse(readFileSync(args.praiseEventsFile, "utf8"));
  }
  payloadOptions.silent = true;

  const payload = await assembleOffchainPayload(payloadOptions);
  const [rewardEpoch] = PublicKey.findProgramAddressSync(
    [Buffer.from("reward_epoch"), epochBuffer],
    PROGRAM_ID
  );
  const [rewardVault] = PublicKey.findProgramAddressSync(
    [Buffer.from("reward_vault"), epochBuffer],
    PROGRAM_ID
  );
  const [rewardVaultAuthority] = PublicKey.findProgramAddressSync(
    [Buffer.from("reward_vault_authority"), epochBuffer],
    PROGRAM_ID
  );

  const proofs = payload.batch.claims.map((claim) => {
    const claimer = new PublicKey(claim.address);
    const [claimStatus] = claimStatusPda(epochBuffer, claimer);
    return {
      address: claim.address,
      amount: claim.amount,
      proof: payload.proofs[claim.address].proof.map((node) => Array.from(node)),
      claimStatus: claimStatus.toBase58(),
    };
  });

  const output = {
    epochId,
    rewardEpoch: rewardEpoch.toBase58(),
    rewardVault: rewardVault.toBase58(),
    rewardVaultAuthority: rewardVaultAuthority.toBase58(),
    totalTokens: payload.batch.totalTokens,
    merkleRoot: payload.batch.merkleRoot,
    proofs,
    issues: payload.issues,
    balances: payload.balances,
    simulation: payload.simulation,
  };

  const serialized = JSON.stringify(output, null, 2);
  if (args.output) {
    writeFileSync(args.output, serialized);
    console.log(`wrote payload snapshot to ${args.output}`);
  }
  console.log(serialized);
}

main().catch((error) => {
  console.error("emit-reward-batch failed", error);
  process.exitCode = 1;
});
