import { readFile } from 'node:fs/promises';

import { describe, expect, test } from 'vite-plus/test';
import { parse } from 'yaml';

async function workflow(path) {
  return parse(await readFile(new URL(path, import.meta.url), 'utf8'));
}

function action(job, uses) {
  const step = job.steps.find((candidate) => candidate.uses?.startsWith(uses));
  expect(step).toBeDefined();
  return step;
}

describe('release workflows', () => {
  test('verification, builds and notes run in parallel before publish', async () => {
    const release = await workflow('../../.github/workflows/release.yml');
    expect(release.jobs.verify.needs).toBe('metadata');
    expect(release.jobs.build.needs).toBe('metadata');
    expect(release.jobs.notes.needs).toBe('metadata');
    expect(release.jobs.publish.needs).toEqual(['metadata', 'verify', 'build', 'notes']);
  });

  test('release notes use a separate artifact directory', async () => {
    const release = await workflow('../../.github/workflows/release.yml');
    const buildUploads = release.jobs.build.steps.filter((step) =>
      step.uses?.startsWith('actions/upload-artifact@'),
    );
    expect(buildUploads).toHaveLength(3);
    for (const upload of buildUploads) {
      expect(upload.with.name).toBe('build-${{ matrix.name }}');
    }

    const upload = action(release.jobs.notes, 'actions/upload-artifact@');
    expect(upload.with).toMatchObject({
      name: 'release-notes',
      path: 'release-notes.md',
    });

    const downloads = release.jobs.publish.steps.filter((step) =>
      step.uses?.startsWith('actions/download-artifact@'),
    );
    expect(downloads).toHaveLength(2);
    expect(downloads[0].with).toMatchObject({
      pattern: 'build-*',
      path: 'downloaded',
      'merge-multiple': true,
    });
    expect(downloads[1].with).toEqual({
      name: 'release-notes',
      path: 'notes',
    });
    expect(release.jobs.publish.steps.find((step) => step.name === 'Stage release').run).toContain(
      '--notes-file notes/release-notes.md',
    );
  });

  test('main warms release caches and tags only restore them', async () => {
    const check = await workflow('../../.github/workflows/check.yml');
    const release = await workflow('../../.github/workflows/release.yml');
    const warm = check.jobs['release-cache'];
    const verifyCache = action(check.jobs['cross-platform'], 'Swatinem/rust-cache@');
    const buildCache = action(release.jobs.build, 'Swatinem/rust-cache@');
    const warmCaches = warm.steps.filter((step) => step.uses?.startsWith('Swatinem/rust-cache@'));

    expect(warm.if).toBe("github.event_name == 'push' && github.ref == 'refs/heads/main'");
    expect(warm.strategy.matrix.include.map(({ name }) => name)).toEqual([
      'macos-arm64',
      'macos-x64',
      'windows-x64',
      'linux-x64',
    ]);
    expect(verifyCache.with).toEqual({
      'save-if': "${{ github.ref == 'refs/heads/main' }}",
    });
    expect(buildCache.with).toEqual({
      'shared-key': 'release-deps',
      'save-if': false,
    });
    expect(warmCaches).toHaveLength(2);
    expect(warmCaches[0]).toMatchObject({
      id: 'release-cache',
      with: {
        'lookup-only': true,
        'save-if': false,
        'shared-key': 'release-deps',
      },
    });
    expect(warmCaches[1]).toMatchObject({
      if: "steps.release-cache.outputs.cache-hit != 'true'",
      with: {
        'save-if': true,
        'shared-key': 'release-deps',
      },
    });
    const pnpmSetup = action(warm, 'pnpm/setup@');
    expect(pnpmSetup.if).toBe("steps.release-cache.outputs.cache-hit != 'true'");
    const linuxDependencies = warm.steps.find((step) =>
      step.run?.includes('libwebkit2gtk-4.1-dev'),
    );
    expect(linuxDependencies.if).toBe(
      "runner.os == 'Linux' && steps.release-cache.outputs.cache-hit != 'true'",
    );
    const targetWarm = warm.steps.find(
      (step) => step.name === 'Warm release dependencies with target',
    );
    expect(targetWarm.if).toBe(
      "steps.release-cache.outputs.cache-hit != 'true' && matrix.target != ''",
    );
    expect(targetWarm.run).toContain('--no-bundle');
  });
});
