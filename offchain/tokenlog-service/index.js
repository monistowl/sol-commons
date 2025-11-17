const config = require('./config.json');

async function fetchGithubIssues() {
  const repo = config.repo;
  if (!repo) {
    return config.mockIssues;
  }
  const perPage = config.maxIssues || 4;
  const url = `https://api.github.com/repos/${repo}/issues?state=open&per_page=${perPage}`;
  try {
    const response = await fetch(url, {
      headers: {
        Accept: 'application/vnd.github+json',
        'User-Agent': 'sol-commons-tokenlog',
      },
    });
    if (!response.ok) {
      throw new Error(`GitHub returned ${response.status}`);
    }
    const data = await response.json();
    const filtered = data.filter((issue) => !issue.pull_request);
    if (!filtered.length) {
      throw new Error('no issues returned');
    }
    return filtered.map((issue) => ({
      id: issue.id,
      title: issue.title,
      url: issue.html_url,
      createdAt: issue.created_at,
      author: issue.user?.login,
      comments: issue.comments,
    }));
  } catch (error) {
    console.warn('tokenlog-service: failing back to mock issues', error.message);
    return config.mockIssues;
  }
}

function sampleBalances() {
  const snapshotDate = new Date().toISOString();
  return config.mockBalances.map((entry) => ({
    ...entry,
    snapshotDate,
  }));
}

module.exports = { fetchGithubIssues, sampleBalances };
