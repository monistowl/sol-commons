const { expect } = require('chai');
const { startPraiseService } = require('../../offchain/praise-service/index');
const { fetchGithubIssues, sampleBalances } = require('../../offchain/tokenlog-service/index');
const { runSimulation } = require('../../offchain/simulator-pipeline/index');

describe('offchain scaffolding integration', () => {
  it('generates reward batch and ties helpers together', async () => {
    const praiseService = startPraiseService();
    praiseService.collect('test-event');
    const batch = praiseService.generateRewardBatch();
    expect(batch).to.have.keys(['merkleRoot', 'totalTokens', 'snapshotDate']);

    const issues = fetchGithubIssues();
    const balances = sampleBalances();
    expect(issues).to.be.an('array').with.length.greaterThan(0);
    expect(balances).to.be.an('array');

    const sim = runSimulation();
    expect(sim.params).to.include.keys(['fundingRatio', 'convictionDecay']);
  });
});
