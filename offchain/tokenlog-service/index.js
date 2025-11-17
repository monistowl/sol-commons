const config = require('./config.json');

function fetchGithubIssues() {
  return config.mockIssues;
}

function sampleBalances() {
  return config.mockBalances;
}

module.exports = { fetchGithubIssues, sampleBalances };
