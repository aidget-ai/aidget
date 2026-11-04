import { appendFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

import { runGit } from '../utils/git.mjs';
import { isNewerTag, isReleaseTag, parseTag } from './version.mjs';

function highestTag(tags) {
  return tags
    .filter(isReleaseTag)
    .reduce((highest, tag) => (!highest || isNewerTag(tag, highest) ? tag : highest), null);
}

/** Allocate a stable patch above every existing release tag, including unpublished betas. */
export function nextPatchTag(tags) {
  const highest = highestTag(tags);
  if (!highest) {
    throw new Error('no release tags; create the first release tag manually');
  }
  const [major, minor, patch] = parseTag(highest).split('-')[0].split('.');
  return `v${major}.${minor}.${BigInt(patch) + 1n}`;
}

async function refresh(root) {
  await runGit(['fetch', '--tags', 'origin', '+refs/heads/main:refs/remotes/origin/main'], root);
}

/** Push an annotated tag only for the checked main HEAD; return a reason when skipped. */
export async function createNextTag(sha, root = process.cwd()) {
  if (!/^[a-f0-9]{40}(?:[a-f0-9]{24})?$/.test(sha)) {
    throw new Error('automatic tagging requires a full checked commit SHA');
  }
  await refresh(root);
  if ((await runGit(['rev-parse', 'refs/remotes/origin/main'], root)).trim() !== sha) {
    return { status: 'stale' };
  }

  const existing = (await runGit(['tag', '--points-at', sha], root)).trim().split('\n');
  if (existing.some(isReleaseTag)) {
    return { status: 'tagged' };
  }
  const tags = (await runGit(['tag', '--list'], root)).trim().split('\n');
  const tag = nextPatchTag(tags);
  // Recheck main immediately before writing; it may have advanced during allocation.
  const remoteMain = await runGit(['ls-remote', '--exit-code', 'origin', 'refs/heads/main'], root);
  if (remoteMain.trim().split(/\s+/)[0] !== sha) {
    return { status: 'stale' };
  }
  await runGit(['tag', '-a', tag, sha, '-m', `aidget ${tag}`], root);
  await runGit(['push', 'origin', `refs/tags/${tag}:refs/tags/${tag}`], root);
  return { status: 'created', tag };
}

/** Refresh remote state and reject publication of a tag superseded by a higher version. */
export async function isCurrentRelease(tag, root = process.cwd()) {
  parseTag(tag);
  await refresh(root);
  const tags = (await runGit(['tag', '--list'], root)).trim().split('\n');
  if (!tags.includes(tag)) {
    throw new Error(`missing release tag ${tag}`);
  }
  return highestTag(tags) === tag;
}

async function main([command, value]) {
  let result;
  if (command === 'auto') {
    result = await createNextTag(value);
  } else if (command === 'current') {
    result = { current: String(await isCurrentRelease(value)) };
  } else {
    throw new Error('usage: tags.mjs <auto SHA|current TAG>');
  }
  console.log(JSON.stringify(result));
  if (process.env.GITHUB_OUTPUT) {
    await appendFile(
      process.env.GITHUB_OUTPUT,
      Object.entries(result)
        .map(([key, output]) => `${key}=${output}\n`)
        .join(''),
    );
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`release tags: ${error.message}`);
    process.exitCode = 1;
  });
}
