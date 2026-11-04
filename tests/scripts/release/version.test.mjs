import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterEach, describe, expect, test } from 'vite-plus/test';

import {
  checkVersion,
  isNewerTag,
  isPrereleaseTag,
  parseTag,
  readVersions,
  rpmBundleConfig,
  setVersion,
} from '../../../scripts/release/version.mjs';

const roots = [];
const CARGO = `[package]
name = "aidget"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0" }
`;

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), 'aidget-version-'));
  roots.push(root);
  await writeFile(join(root, 'Cargo.toml'), CARGO);
  await writeFile(join(root, 'package.json'), '{\n  "name": "aidget",\n  "version": "0.1.0"\n}\n');
  await writeFile(
    join(root, 'tauri.conf.json'),
    '{\n  "productName": "aidget",\n  "version": "0.1.0"\n}\n',
  );
  return root;
}

describe('release version', () => {
  test('parseTag accepts stable and numbered beta tags', () => {
    expect(parseTag('v0.2.0')).toBe('0.2.0');
    expect(parseTag('v0.1.1-beta.0')).toBe('0.1.1-beta.0');
    expect(parseTag('v0.1.1-beta.12')).toBe('0.1.1-beta.12');
    for (const bad of [
      '0.2.0',
      'v01.2.0',
      'v0.2',
      'v0.2.0-beta',
      'v0.2.0-beta.01',
      'v0.2.0-beta.-1',
      'v0.2.0+build',
    ]) {
      expect(() => parseTag(bad)).toThrow();
    }
  });

  test('isPrereleaseTag distinguishes beta from stable releases', () => {
    expect(isPrereleaseTag('v0.1.1-beta.0')).toBe(true);
    expect(isPrereleaseTag('v0.1.1')).toBe(false);
    expect(() => isPrereleaseTag('invalid')).toThrow();
  });

  test('isNewerTag compares stable versions numerically', () => {
    expect(isNewerTag('v0.10.0', 'v0.9.9')).toBe(true);
    expect(isNewerTag('v1.0.0', 'v0.99.99')).toBe(true);
    expect(isNewerTag('v0.2.0', 'v0.2.0')).toBe(false);
    expect(isNewerTag('v0.2.0', 'v0.3.0')).toBe(false);
  });

  test('isNewerTag orders beta sequences before the corresponding stable release', () => {
    expect(isNewerTag('v0.1.1-beta.0', 'v0.1.0')).toBe(true);
    expect(isNewerTag('v0.1.1-beta.10', 'v0.1.1-beta.9')).toBe(true);
    expect(isNewerTag('v0.1.1-beta.0', 'v0.1.1-beta.0')).toBe(false);
    expect(isNewerTag('v0.1.1-beta.0', 'v0.1.1')).toBe(false);
    expect(isNewerTag('v0.1.1', 'v0.1.1-beta.0')).toBe(true);
    expect(isNewerTag('v0.1.0', 'v0.1.1-beta.0')).toBe(false);
  });

  test('setVersion injects beta versions into all manifests', async () => {
    const root = await fixture();
    await setVersion(root, 'v0.1.1-beta.0');
    expect(await readVersions(root)).toEqual({
      cargo: '0.1.1-beta.0',
      packageJson: '0.1.1-beta.0',
      tauriConfig: '0.1.1-beta.0',
    });
    await checkVersion(root, 'v0.1.1-beta.0');
  });

  test('rpmBundleConfig keeps beta ordering in the RPM release field', () => {
    expect(rpmBundleConfig('v0.1.1-beta.0')).toEqual({
      version: '0.1.1',
      bundle: { linux: { rpm: { release: '0.beta.0.1' } } },
    });
    expect(rpmBundleConfig('v0.1.1-beta.10')).toEqual({
      version: '0.1.1',
      bundle: { linux: { rpm: { release: '0.beta.10.1' } } },
    });
    expect(rpmBundleConfig('v0.1.1')).toEqual({
      version: '0.1.1',
      bundle: { linux: { rpm: { release: '1' } } },
    });
    expect(() => rpmBundleConfig('invalid')).toThrow();
  });

  test('setVersion updates all three manifests and leaves dependency versions alone', async () => {
    const root = await fixture();

    await setVersion(root, 'v0.2.3');

    expect(await readVersions(root)).toEqual({
      cargo: '0.2.3',
      packageJson: '0.2.3',
      tauriConfig: '0.2.3',
    });
    expect(await readFile(join(root, 'Cargo.toml'), 'utf8')).toMatch(
      /serde = \{ version = "1\.0" \}/,
    );
    await checkVersion(root, 'v0.2.3');
  });

  test('checkVersion rejects a manifest that does not match the tag', async () => {
    const root = await fixture();

    await expect(checkVersion(root, 'v0.2.0')).rejects.toThrow(/cargo version is 0\.1\.0/);
  });
});
