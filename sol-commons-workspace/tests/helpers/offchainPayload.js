const fs = require('fs');
const { assembleOffchainPayload } = require('../../../offchain/pipeline/index');

let cachedFilePayload = null;

async function loadOffchainPayload(options = {}) {
  const payloadPath = process.env.OFFCHAIN_PIPELINE_PAYLOAD;
  if (payloadPath && fs.existsSync(payloadPath)) {
    if (!cachedFilePayload) {
      cachedFilePayload = JSON.parse(fs.readFileSync(payloadPath, 'utf8'));
    }
    return cachedFilePayload;
  }
  return assembleOffchainPayload(options);
}

module.exports = { loadOffchainPayload };
