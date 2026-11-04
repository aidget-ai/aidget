import { pathToFileURL } from 'node:url';

import { isNewerTag, isReleaseTag, parseTag } from './version.mjs';

/** Cancel active lower-version Release runs; a late or rerun old tag cannot cancel a newer one. */
export async function cancelOlderReleases({ tag, repository, token, fetch = globalThis.fetch }) {
  parseTag(tag);
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository) || !token) {
    throw new Error('release cancellation requires owner/repository and an Actions token');
  }
  const base = `https://api.github.com/repos/${repository}/actions`;
  const headers = {
    Authorization: `Bearer ${token}`,
    Accept: 'application/vnd.github+json',
    'X-GitHub-Api-Version': '2022-11-28',
  };
  const runs = [];
  // Collect all pages before cancellation mutates the runs' states.
  for (let page = 1; ; page += 1) {
    const response = await fetch(
      `${base}/workflows/release.yml/runs?event=push&per_page=100&page=${page}`,
      { headers, redirect: 'error', signal: AbortSignal.timeout(30_000) },
    );
    if (!response.ok) {
      throw new Error(`GitHub API returned ${response.status} while listing release runs`);
    }
    const { workflow_runs: batch } = await response.json();
    if (!Array.isArray(batch)) {
      throw new Error('GitHub API returned invalid release runs');
    }
    runs.push(...batch);
    if (batch.length < 100) {
      break;
    }
  }
  let cancelled = 0;
  for (const run of runs) {
    if (
      run.event !== 'push' ||
      run.status === 'completed' ||
      !isReleaseTag(run.head_branch) ||
      !isNewerTag(tag, run.head_branch)
    ) {
      continue;
    }
    if (!Number.isSafeInteger(run.id) || run.id <= 0) {
      throw new Error('GitHub API returned an invalid workflow run ID');
    }
    const response = await fetch(`${base}/runs/${run.id}/cancel`, {
      method: 'POST',
      headers,
      redirect: 'error',
      signal: AbortSignal.timeout(30_000),
    });
    if (response.status === 202) {
      cancelled += 1;
      continue;
    }
    // A run can finish between listing and cancellation; only that conflict is benign.
    if (response.status === 409) {
      const current = await fetch(`${base}/runs/${run.id}`, {
        headers,
        redirect: 'error',
        signal: AbortSignal.timeout(30_000),
      });
      if (current.ok && (await current.json()).status === 'completed') {
        continue;
      }
    }
    throw new Error(
      `GitHub API returned ${response.status} while cancelling release run ${run.id}`,
    );
  }
  return cancelled;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  cancelOlderReleases({
    tag: process.env.RELEASE_TAG,
    repository: process.env.GITHUB_REPOSITORY,
    token: process.env.GH_TOKEN,
  })
    .then((count) => console.log(`Cancelled ${count} older release runs`))
    .catch((error) => {
      console.error(`release cancellation: ${error.message}`);
      process.exitCode = 1;
    });
}
