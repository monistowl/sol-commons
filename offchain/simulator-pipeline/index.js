const config = require('./config.json');

function runSimulation() {
  return {
    scenario: config.scenario,
    timestamp: Date.now(),
    params: { fundingRatio: 0.6, convictionDecay: 0.5 },
  };
}

module.exports = { runSimulation };
