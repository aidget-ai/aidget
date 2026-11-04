import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterEach, describe, expect, test, vi } from 'vite-plus/test';

import { createNextTag, isCurrentRelease, nextPatchTag } from '../../../scripts/release/tags.mjs';
import { runGit } from '../../../scripts/utils/git.mjs';
import * as git from '../../../scripts/utils/git.mjs';

const roots = [];

afterEach(async () => {
  vi.restoreAllMocks();
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), 'aidget-tags-'));
  roots.push(root);
  const remote = join(root, 'remote.git');
  const work = join(root, 'work');
  await runGit(['init', '--bare', remote]);
  await mkdir(work);
  await runGit(['init', '-b', 'main'], work);
  await runGit(['config', 'user.name', 'Release Test'], work);
  await runGit(['config', 'user.email', 'release@example.test'], work);
  await runGit(['config', 'tag.gpgSign', 'false'], work);
  await runGit(['commit', '--allow-empty', '-m', 'initial'], work);
  await runGit(['tag', '-a', 'v0.1.3', '-m', 'initial release'], work);
  await runGit(['remote', 'add', 'origin', remote], work);
  await runGit(['push', 'origin', 'main', '--tags'], work);
  await runGit(['commit', '--allow-empty', '-m', 'next change'], work);
  await runGit(['push', 'origin', 'main'], work);
  const sha = (await runGit(['rev-parse', 'HEAD'], work)).trim();
  return { work, remote, sha };
}

describe('release tags', () => {
  test('increments the highest numeric core patch, including failed and beta tags', () => {
    expect(nextPatchTag(['v0.1.9', 'v0.1.10', 'v0.1.11-beta.2', 'other', 'v01.2.3'])).toBe(
      'v0.1.12',
    );
    expect(nextPatchTag(['v1.0.0-beta.0', 'v0.99.99'])).toBe('v1.0.1');
    expect(nextPatchTag(['v0.1.9007199254740993'])).toBe('v0.1.9007199254740994');
    expect(() => nextPatchTag(['unrelated'])).toThrow('no release tags');
  });

  test('pushes an annotated tag on the checked SHA and is idempotent', async () => {
    const { work, remote, sha } = await fixture();
    expect(await createNextTag(sha, work)).toEqual({ status: 'created', tag: 'v0.1.4' });
    expect((await runGit(['cat-file', '-t', 'refs/tags/v0.1.4'], remote)).trim()).toBe('tag');
    expect((await runGit(['rev-parse', 'v0.1.4^{}'], remote)).trim()).toBe(sha);
    expect(await createNextTag(sha, work)).toEqual({ status: 'tagged' });
    expect((await runGit(['tag', '--list'], remote)).trim().split('\n')).toEqual([
      'v0.1.3',
      'v0.1.4',
    ]);
  });

  test('skips a checked commit when remote main has advanced', async () => {
    const { work, remote, sha } = await fixture();
    await runGit(['commit', '--allow-empty', '-m', 'newer change'], work);
    await runGit(['push', 'origin', 'main'], work);
    expect(await createNextTag(sha, work)).toEqual({ status: 'stale' });
    expect((await runGit(['tag', '--list'], remote)).trim()).toBe('v0.1.3');
  });

  test('a manual beta tag on the same commit suppresses the automatic stable tag', async () => {
    const { work, sha } = await fixture();
    await runGit(['tag', '-a', 'v0.1.4-beta.0', '-m', 'manual beta'], work);
    await runGit(['push', 'origin', 'v0.1.4-beta.0'], work);
    expect(await createNextTag(sha, work)).toEqual({ status: 'tagged' });
  });

  test('rechecks main if it advances after the initial fetch', async () => {
    const { work, remote, sha } = await fixture();
    const original = git.runGit;
    vi.spyOn(git, 'runGit').mockImplementation(async (args, cwd) => {
      if (args[0] === 'ls-remote') {
        await original(['commit', '--allow-empty', '-m', 'advance during allocation'], work);
        await original(['push', 'origin', 'main'], work);
      }
      return original(args, cwd);
    });
    expect(await createNextTag(sha, work)).toEqual({ status: 'stale' });
    expect((await original(['tag', '--list'], remote)).trim()).toBe('v0.1.3');
  });

  test('refreshes remote tags before allocating a version', async () => {
    const { work, remote, sha } = await fixture();
    await runGit(['tag', 'v0.1.4', 'main~1'], remote);
    expect(await createNextTag(sha, work)).toEqual({ status: 'created', tag: 'v0.1.5' });
    expect((await runGit(['rev-parse', 'v0.1.4'], remote)).trim()).not.toBe(sha);
  });

  test('a rejected push fails without replacing any remote tag', async () => {
    const { work, remote, sha } = await fixture();
    await writeFile(join(remote, 'hooks', 'pre-receive'), '#!/bin/sh\nexit 1\n', { mode: 0o755 });
    await expect(createNextTag(sha, work)).rejects.toThrow();
    expect((await runGit(['tag', '--list'], remote)).trim()).toBe('v0.1.3');
  });

  test('a concurrently created remote tag wins without being overwritten or retried', async () => {
    const { work, remote, sha } = await fixture();
    await writeFile(
      join(work, '.git', 'hooks', 'pre-push'),
      '#!/bin/sh\ngit --git-dir="$2" tag v0.1.4 main~1\n',
      { mode: 0o755 },
    );
    await expect(createNextTag(sha, work)).rejects.toThrow();
    expect((await runGit(['rev-parse', 'v0.1.4'], remote)).trim()).not.toBe(sha);
    expect((await runGit(['tag', '--list'], remote)).trim().split('\n')).toEqual([
      'v0.1.3',
      'v0.1.4',
    ]);
  });

  test('requires an explicit initial version and a full checked SHA', async () => {
    const { work, remote, sha } = await fixture();
    await runGit(['tag', '-d', 'v0.1.3'], work);
    await runGit(['tag', '-d', 'v0.1.3'], remote);
    await expect(createNextTag(sha, work)).rejects.toThrow('no release tags');
    await expect(createNextTag('main', work)).rejects.toThrow('full checked commit SHA');
    await expect(createNextTag('--all', work)).rejects.toThrow('full checked commit SHA');
  });

  test('only the highest supported remote tag may publish, including on old reruns', async () => {
    const { work, remote } = await fixture();
    expect(await isCurrentRelease('v0.1.3', work)).toBe(true);
    await runGit(['tag', 'v0.1.4-beta.0', 'main'], remote);
    expect(await isCurrentRelease('v0.1.3', work)).toBe(false);
    expect(await isCurrentRelease('v0.1.4-beta.0', work)).toBe(true);
    await expect(isCurrentRelease('v9.0.0', work)).rejects.toThrow('missing release tag');
    await expect(isCurrentRelease('invalid', work)).rejects.toThrow('release tag must');
  });
});
