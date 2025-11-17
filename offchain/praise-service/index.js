const EventEmitter = require('events');
const config = require('./config.json');

class PraiseService extends EventEmitter {
  constructor() {
    super();
    this.config = config;
  }

  collect(event) {
    this.emit('praise', { event, timestamp: Date.now() });
  }

  generateRewardBatch() {
    return {
      merkleRoot: Buffer.alloc(32).toString('hex'),
      totalTokens: this.config.defaultReward,
      snapshotDate: new Date().toISOString(),
    };
  }
}

function startPraiseService() {
  const service = new PraiseService();
  service.on('praise', (payload) => {
    console.log('praise received', payload);
  });
  return service;
}

module.exports = { startPraiseService };
