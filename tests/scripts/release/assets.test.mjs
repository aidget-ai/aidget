import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterEach, describe, expect, test } from 'vite-plus/test';

import { assetName, collectAssets } from '../../../scripts/release/assets.mjs';

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
    for (const [source, expected] of [
      ['aidget_0.2.0_aarch64.dmg', 'aidget-darwin-arm64.dmg'],
      ['aidget_0.2.0_x64.dmg', 'aidget-darwin-amd64.dmg'],
      ['aidget-darwin-arm64.zip', 'aidget-darwin-arm64.zip'],
      ['aidget-darwin-amd64.zip', 'aidget-darwin-amd64.zip'],
      ['aidget_0.2.0_x64-setup.exe', 'aidget-windows-amd64.exe'],
      ['aidget_0.2.0_arm64-setup.exe', 'aidget-windows-arm64.exe'],
      ['aidget_0.2.0_amd64.deb', 'aidget-linux-amd64.deb'],
      ['aidget_0.2.0_arm64.deb', 'aidget-linux-arm64.deb'],
      ['aidget-0.2.0-1.x86_64.rpm', 'aidget-linux-amd64.rpm'],
      ['aidget-0.2.0-1.aarch64.rpm', 'aidget-linux-arm64.rpm'],
    ]) {
      expect(assetName(source)).toBe(expected);
    }
    for (const bad of [
      'notes.txt',
      'other_0.2.0_x64.dmg',
      'aidget_0.2.0_ia32-setup.exe',
      'aidget_0.2.0_amd64.AppImage',
      'aidget_0.2.0_riscv64.deb',
      'aidget-0.2.0-1.riscv64.rpm',
      'aidget-darwin-arm64.zip.bak',
    ]) {
      expect(() => assetName(bad)).toThrow();
    }
  });

  test.each(['0.2.0', '0.1.1-beta.0'])(
    'collectAssets copies all %s packages and writes SHA256SUMS',
    async (version) => {
      const root = await tempRoot('aidget-assets-');
      const input = join(root, 'input');
      const output = join(root, 'output');
      await mkdir(join(input, 'dmg'), { recursive: true });
      await mkdir(join(input, 'appzip'), { recursive: true });
      await mkdir(join(input, 'nsis'), { recursive: true });
      await mkdir(join(input, 'deb'), { recursive: true });
      await mkdir(join(input, 'rpm'), { recursive: true });

      const sources = [
        ['dmg/aidget_0.2.0_aarch64.dmg', 'arm'],
        ['dmg/aidget_0.2.0_x64.dmg', 'intel'],
        ['appzip/aidget-darwin-arm64.zip', 'arm-app'],
        ['appzip/aidget-darwin-amd64.zip', 'intel-app'],
        ['nsis/aidget_0.2.0_x64-setup.exe', 'windows'],
        ['nsis/aidget_0.2.0_arm64-setup.exe', 'windows-arm'],
        ['deb/aidget_0.2.0_amd64.deb', 'deb'],
        ['deb/aidget_0.2.0_arm64.deb', 'deb-arm'],
        ['rpm/aidget-0.2.0-1.x86_64.rpm', 'rpm'],
        ['rpm/aidget-0.2.0-1.aarch64.rpm', 'rpm-arm'],
      ];
      for (const [path, body] of sources) {
        await writeFile(join(input, path.replaceAll('0.2.0', version)), body);
      }

      const names = await collectAssets({ input, output });
      expect(names).toHaveLength(10);
      expect(await readFile(join(output, 'aidget-linux-amd64.rpm'), 'utf8')).toBe('rpm');
      expect(await readFile(join(output, 'aidget-linux-arm64.rpm'), 'utf8')).toBe('rpm-arm');
      expect(await readFile(join(output, 'SHA256SUMS'), 'utf8')).toContain(
        'aidget-windows-amd64.exe',
      );
      expect(await readFile(join(output, 'SHA256SUMS'), 'utf8')).toContain(
        'aidget-windows-arm64.exe',
      );
    },
  );

  test('collectAssets rejects duplicate packages', async () => {
    const root = await tempRoot('aidget-assets-duplicate-');
    const input = join(root, 'input');
    await mkdir(input, { recursive: true });
    await writeFile(join(input, 'aidget_0.2.0_aarch64.dmg'), 'one');
    await writeFile(join(input, 'aidget_0.3.0_aarch64.dmg'), 'two');
    await expect(collectAssets({ input, output: join(root, 'output') })).rejects.toThrow(
      'two source files map to aidget-darwin-arm64.dmg',
    );
  });

  test('collectAssets fails when a required platform is missing', async () => {
    const root = await tempRoot('aidget-assets-missing-');
    const input = join(root, 'input');
    await mkdir(input, { recursive: true });
    await writeFile(join(input, 'aidget_0.2.0_aarch64.dmg'), 'arm');

    await expect(collectAssets({ input, output: join(root, 'output') })).rejects.toThrow(
      /missing release asset/,
    );
  });
});
