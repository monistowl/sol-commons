const crypto = require('crypto');
const config = require('./config.json');

function deterministicRandom(seed = Date.now()) {
  const hash = crypto.createHash('sha256').update(String(seed)).digest();
  const value = hash.readUInt32LE(0);
  return (value % 1000) / 1000;
}

function runSimulation(options = {}) {
  const seed = options.seed ?? Date.now();
  const randomness = deterministicRandom(seed);
  const base = config.baseParams ?? {};
  const params = {
    fundingRatio: Number(((base.fundingRatio ?? 0.6) + randomness * 0.2).toFixed(3)),
    convictionDecay: Number(((base.convictionDecay ?? 0.5) - randomness * 0.1).toFixed(3)),
    rewardMultiplier: Number(((base.rewardMultiplier ?? 1.2) + randomness * 0.3).toFixed(3)),
  };
  const metrics = {
    confidence: Number((0.55 + randomness * 0.45).toFixed(3)),
    projectedPayout: Math.round(params.fundingRatio * 1_000_000),
    throughput: Math.round((config.iterations ?? 1000) * (0.5 + randomness)),
  };
  return {
    scenario: config.scenario,
    timestamp: new Date().toISOString(),
    params,
    metrics,
    iterations: config.iterations,
    seed,
  };
}

module.exports = { runSimulation };
