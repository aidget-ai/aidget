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
  test('auto tagging waits for successful main push checks and uses the bot identity', async () => {
    const automatic = await workflow('../../../.github/workflows/auto-tag.yml');
    expect(automatic.on).toEqual({
      workflow_run: { workflows: ['Check'], types: ['completed'], branches: ['main'] },
    });
    expect(automatic.concurrency).toEqual({ group: 'auto-tag', 'cancel-in-progress': false });
    const job = automatic.jobs.tag;
    expect(job.if).toContain("github.event.workflow_run.conclusion == 'success'");
    expect(job.if).toContain("github.event.workflow_run.event == 'push'");
    expect(job.if).toContain(
      'github.event.workflow_run.head_repository.full_name == github.repository',
    );
    expect(action(job, 'actions/checkout@').with.ref).toBe(
      '${{ github.event.workflow_run.head_sha }}',
    );
    expect(action(job, 'actions/checkout@').with.token).toBe(
      '${{ steps.bot-token.outputs.token }}',
    );
    expect(action(job, 'actions/create-github-app-token@').with['permission-contents']).toBe(
      'write',
    );
  });

  test('obsolete tags cannot cancel newer releases or stage and publish a draft', async () => {
    const release = await workflow('../../../.github/workflows/release.yml');
    expect(release.concurrency['cancel-in-progress']).toBe(false);
    expect(release.jobs.metadata.permissions.actions).toBe('write');
    expect(release.jobs.metadata.outputs.current).toBe('${{ steps.current.outputs.current }}');
    for (const name of ['verify', 'build', 'notes']) {
      expect(release.jobs[name].if).toBe("needs.metadata.outputs.current == 'true'");
    }
    const cancel = release.jobs.metadata.steps.find(
      (step) => step.name === 'Cancel older releases',
    );
    expect(cancel.if).toBe("steps.current.outputs.current == 'true'");
    expect(cancel.run).toBe('node scripts/release/cancel.mjs');
    const steps = release.jobs.publish.steps;
    for (const [name, guard] of [
      ['Stage release', 'before-stage'],
      ['Publish release', 'before-publish'],
    ]) {
      const index = steps.findIndex((step) => step.name === name);
      expect(steps[index - 1].id).toBe(guard);
      expect(steps[index - 1].run).toBe('node scripts/release/tags.mjs current "$TAG"');
      expect(steps[index].if).toBe(`steps.${guard}.outputs.current == 'true'`);
    }
  });

  test('verification, builds and notes run in parallel before publish', async () => {
    const release = await workflow('../../../.github/workflows/release.yml');
    expect(release.jobs.verify.needs).toBe('metadata');
    expect(release.jobs.build.needs).toBe('metadata');
    expect(release.jobs.notes.needs).toBe('metadata');
    expect(release.jobs.publish.needs).toEqual(['metadata', 'verify', 'build', 'notes']);
  });

  test('release builds all six native targets and publishes eleven files', async () => {
    const release = await workflow('../../../.github/workflows/release.yml');
    const targets = release.jobs.build.strategy.matrix.include;

    expect(targets.map(({ name }) => name)).toEqual([
      'macos-arm64',
      'macos-x64',
      'windows-x64',
      'windows-arm64',
      'linux-x64',
      'linux-arm64',
    ]);
    expect(targets.find(({ name }) => name === 'windows-arm64')).toMatchObject({
      os: 'windows-11-arm',
      target: '',
      bundles: 'nsis',
      pe_machine: 'aa64',
    });
    expect(targets.find(({ name }) => name === 'windows-x64')).toMatchObject({
      pe_machine: '8664',
    });
    expect(targets.find(({ name }) => name === 'linux-arm64')).toMatchObject({
      os: 'ubuntu-22.04-arm',
      target: '',
      bundles: 'deb',
    });
    expect(release.jobs.publish.steps.find((step) => step.name === 'Stage release').run).toContain(
      '-eq 11',
    );
  });

  test('release notes use a separate artifact directory', async () => {
    const release = await workflow('../../../.github/workflows/release.yml');
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
    const check = await workflow('../../../.github/workflows/check.yml');
    const release = await workflow('../../../.github/workflows/release.yml');
    const warm = check.jobs['release-cache'];
    const verifyCache = action(check.jobs['cross-platform'], 'Swatinem/rust-cache@');
    const buildCache = action(release.jobs.build, 'Swatinem/rust-cache@');
    const warmCaches = warm.steps.filter((step) => step.uses?.startsWith('Swatinem/rust-cache@'));

    expect(warm.if).toBe("github.event_name == 'push' && github.ref == 'refs/heads/main'");
    expect(warm.strategy.matrix.include.map(({ name }) => name)).toEqual([
      'macos-arm64',
      'macos-x64',
      'windows-x64',
      'windows-arm64',
      'linux-x64',
      'linux-arm64',
    ]);
    expect(check.jobs['cross-platform'].strategy.matrix.os).toEqual([
      'macos-latest',
      'windows-2022',
      'windows-11-arm',
      'ubuntu-22.04',
      'ubuntu-22.04-arm',
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
