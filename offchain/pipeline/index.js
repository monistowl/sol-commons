const fs = require('fs');
const { startPraiseService } = require('../praise-service/index');
const { fetchGithubIssues, sampleBalances } = require('../tokenlog-service/index');
const { runSimulation } = require('../simulator-pipeline/index');

async function assembleOffchainPayload(options = {}) {
  const praiseEvents = Array.isArray(options.praiseEvents) ? options.praiseEvents : [];
  const quiet = options.silent === true || process.env.SOL_COMMONS_PIPELINE_SILENT === '1';
  const praiseService = startPraiseService({ silent: quiet });
  praiseEvents.forEach((event) => praiseService.collect(event));

  const batch = praiseService.generateRewardBatch();
  const issues = await fetchGithubIssues();
  const balances = sampleBalances();
  const simulation = runSimulation(options.simulation);

  const proofs = {};
  batch.claims.forEach((claim) => {
    proofs[claim.address] = praiseService.proofFor(claim.address);
  });

  return {
    batch,
    proofs,
    issues,
    balances,
    simulation,
    praiseEvents,
  };
}

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--praise-events-file' && i + 1 < argv.length) {
      args.praiseEventsFile = argv[++i];
      continue;
    }
    if (arg === '--help' || arg === '-h') {
      args.help = true;
      continue;
    }
  }
  return args;
}

if (require.main === module) {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    console.log('Usage: node pipeline/index.js [--praise-events-file path]');
    process.exit(0);
  }
  const options = {};
  if (args.praiseEventsFile) {
    options.praiseEvents = JSON.parse(fs.readFileSync(args.praiseEventsFile, 'utf8'));
  }
  assembleOffchainPayload(options)
    .then((payload) => {
      process.stdout.write(JSON.stringify(payload));
    })
    .catch((error) => {
      console.error('offchain pipeline failed', error);
      process.exitCode = 1;
    });
}

module.exports = { assembleOffchainPayload };
