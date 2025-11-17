const crypto = require('crypto');
const EventEmitter = require('events');
const config = require('./config.json');

const DEFAULT_ROOT = Buffer.alloc(32);
const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const BASE58_BASE = BigInt(58);

function sha256(buffer) {
  return crypto.createHash('sha256').update(buffer).digest();
}

function base58Decode(value) {
  let num = BigInt(0);
  for (const char of value) {
    const digit = BigInt(BASE58_ALPHABET.indexOf(char));
    if (digit < 0) {
      throw new Error(`Invalid base58 character ${char}`);
    }
    num = num * BASE58_BASE + digit;
  }

  const bytes = [];
  while (num > 0) {
    bytes.push(Number(num & BigInt(0xff)));
    num >>= BigInt(8);
  }

  for (let i = 0; i < value.length && value[i] === '1'; i += 1) {
    bytes.unshift(0);
  }

  const buffer = Buffer.from(bytes);
  if (buffer.length >= 32) {
    return buffer;
  }
  return Buffer.concat([Buffer.alloc(32 - buffer.length, 0), buffer]);
}

function hashLeaf(address, amount) {
  const addressBuffer = base58Decode(address);
  const amountBuffer = Buffer.alloc(8);
  amountBuffer.writeBigUInt64LE(BigInt(amount));
  return sha256(Buffer.concat([addressBuffer, amountBuffer]));
}

function hashPair(left, right) {
  const [first, second] = Buffer.compare(left, right) <= 0 ? [left, right] : [right, left];
  return sha256(Buffer.concat([first, second]));
}

function buildLayers(leaves) {
  const layers = [];
  let current = leaves.length ? leaves.slice() : [DEFAULT_ROOT];
  layers.push(current);
  while (current.length > 1) {
    const next = [];
    for (let i = 0; i < current.length; i += 2) {
      const left = current[i];
      const right = i + 1 < current.length ? current[i + 1] : current[i];
      next.push(hashPair(left, right));
    }
    current = next;
    layers.push(current);
  }
  return layers;
}

function buildProof(layers, index) {
  const proof = [];
  let idx = index;
  for (let layerIndex = 0; layerIndex < layers.length - 1; layerIndex += 1) {
    const layer = layers[layerIndex];
    const isRight = idx % 2 === 1;
    const siblingIndex = isRight ? idx - 1 : idx + 1;
    if (siblingIndex < layer.length) {
      proof.push(layer[siblingIndex]);
    } else {
      proof.push(layer[idx]);
    }
    idx = Math.floor(idx / 2);
  }
  return proof;
}

class PraiseService extends EventEmitter {
  constructor() {
    super();
    this.config = config;
    this.scoreboard = new Map();
    (Array.isArray(this.config.defaultClaims) ? this.config.defaultClaims : []).forEach(
      ({ address, amount }) => {
        this.scoreboard.set(address, { score: amount, events: [] });
      }
    );
    this.latestBatch = null;
  }

  collect(input = {}) {
    const timestamp = Date.now();
    const payload =
      typeof input === 'string'
        ? { event: input, amount: this.config.defaultReward / 10 || 1 }
        : input;
    const address = payload.address || this.config.defaultClaims?.[0]?.address;
    const amount = payload.amount || Math.max(1, Math.floor(this.config.defaultReward / 10));
    const eventData = { ...payload, address, amount, timestamp };
    const entry = this.scoreboard.get(address) ?? { score: 0, events: [] };
    entry.score += amount;
    entry.events.push(eventData);
    this.scoreboard.set(address, entry);
    this.emit('praise', eventData);
    return eventData;
  }

  computeClaims() {
    const entries = Array.from(this.scoreboard.entries()).map(([address, data]) => ({
      address,
      score: data.score,
    }));
    if (!entries.length) {
      return [];
    }
    const totalScore = entries.reduce((sum, entry) => sum + entry.score, 0);
    const rewardPool = this.config.defaultReward ?? totalScore;
    let remaining = rewardPool;
    return entries.map((entry, index) => {
      const share =
        index === entries.length - 1
          ? remaining
          : Math.max(1, Math.floor((entry.score / totalScore) * rewardPool));
      remaining -= share;
      return { address: entry.address, amount: share };
    });
  }

  generateRewardBatch() {
    const claims = this.computeClaims();
    const leaves = claims.map(({ address, amount }) => hashLeaf(address, amount));
    const layers = buildLayers(leaves);
    const root = layers[layers.length - 1][0] || DEFAULT_ROOT;
    this.latestBatch = { layers, claims };
    return {
      merkleRoot: root.toString('hex'),
      totalTokens: claims.reduce((sum, claim) => sum + claim.amount, 0),
      snapshotDate: new Date().toISOString(),
      claims,
    };
  }

  proofFor(address) {
    if (!this.latestBatch) {
      throw new Error('Reward batch missing; call generateRewardBatch first.');
    }
    const index = this.latestBatch.claims.findIndex((claim) => claim.address === address);
    if (index === -1) {
      throw new Error(`Claim not found for ${address}`);
    }
    const proof = buildProof(this.latestBatch.layers, index);
    return {
      amount: this.latestBatch.claims[index].amount,
      proof,
    };
  }
}

function startPraiseService(options = {}) {
  const service = new PraiseService();
  const shouldLog = !options.silent && process.env.SOL_COMMONS_PRAISE_SILENT !== '1';
  if (shouldLog) {
    service.on('praise', (payload) => {
      console.log('praise received', payload);
    });
  }
  return service;
}

module.exports = { startPraiseService };
