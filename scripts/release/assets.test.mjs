import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterEach, describe, expect, test } from 'vite-plus/test';

import { assetName, collectAssets } from './assets.mjs';

const roots = [];

afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

async function tempRoot(prefix) {
  const root = await mkdtemp(join(tmpdir(), prefix));
  roots.push(root);
  return root;
}

describe('release assets', () => {
  test('assetName normalizes Tauri default bundle names', () => {
    expect(assetName('aidget_0.2.0_aarch64.dmg', '0.2.0')).toBe('aidget_0.2.0_aarch64.dmg');
    expect(assetName('aidget_0.2.0_x64.dmg', '0.2.0')).toBe('aidget_0.2.0_x64.dmg');
    expect(assetName('aidget_0.2.0_aarch64.app.zip', '0.2.0')).toBe('aidget_0.2.0_aarch64.app.zip');
    expect(assetName('aidget_0.2.0_x64.app.zip', '0.2.0')).toBe('aidget_0.2.0_x64.app.zip');
    expect(assetName('aidget-0.2.0-1.x86_64.rpm', '0.2.0')).toBe('aidget_0.2.0_amd64.rpm');
    expect(() => assetName('notes.txt', '0.2.0')).toThrow();
  });

  test('collectAssets copies the expected set and writes SHA256SUMS', async () => {
    const root = await tempRoot('aidget-assets-');
    const input = join(root, 'input');
    const output = join(root, 'output');
    await mkdir(join(input, 'dmg'), { recursive: true });
    await mkdir(join(input, 'appzip'), { recursive: true });
    await mkdir(join(input, 'nsis'), { recursive: true });
    await mkdir(join(input, 'appimage'), { recursive: true });
    await mkdir(join(input, 'deb'), { recursive: true });
    await mkdir(join(input, 'rpm'), { recursive: true });

    const sources = [
      ['dmg/aidget_0.2.0_aarch64.dmg', 'arm'],
      ['dmg/aidget_0.2.0_x64.dmg', 'intel'],
      ['appzip/aidget_0.2.0_aarch64.app.zip', 'arm-app'],
      ['appzip/aidget_0.2.0_x64.app.zip', 'intel-app'],
      ['nsis/aidget_0.2.0_x64-setup.exe', 'windows'],
      ['appimage/aidget_0.2.0_amd64.AppImage', 'appimage'],
      ['deb/aidget_0.2.0_amd64.deb', 'deb'],
      ['rpm/aidget-0.2.0-1.x86_64.rpm', 'rpm'],
    ];
    for (const [path, body] of sources) {
      await writeFile(join(input, path), body);
    }

    const names = await collectAssets({ input, output, version: '0.2.0' });
    expect(names).toHaveLength(8);
    expect(await readFile(join(output, 'aidget_0.2.0_amd64.rpm'), 'utf8')).toBe('rpm');
    expect(await readFile(join(output, 'SHA256SUMS'), 'utf8')).toMatch(
      /aidget_0\.2\.0_x64-setup\.exe/,
    );
  });

  test('collectAssets fails when a required platform is missing', async () => {
    const root = await tempRoot('aidget-assets-missing-');
    const input = join(root, 'input');
    await mkdir(input, { recursive: true });
    await writeFile(join(input, 'aidget_0.2.0_aarch64.dmg'), 'arm');

    await expect(
      collectAssets({ input, output: join(root, 'output'), version: '0.2.0' }),
    ).rejects.toThrow(/missing release asset/);
  });
});
