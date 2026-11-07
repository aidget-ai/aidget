import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { parseTag } from './version.mjs';

/** The complete set uploaded with SHA256SUMS for every release. */
export const RELEASE_ASSETS = [
  'aidget-darwin-arm64.dmg',
  'aidget-darwin-amd64.dmg',
  'aidget-darwin-arm64.zip',
  'aidget-darwin-amd64.zip',
  'aidget-windows-amd64.exe',
  'aidget-windows-arm64.exe',
  'aidget-linux-amd64.deb',
  'aidget-linux-amd64.rpm',
  'aidget-linux-arm64.deb',
  'aidget-linux-arm64.rpm',
];

/** Normalize build output names to the public platform/architecture contract. */
export function assetName(source) {
  const name = basename(source);
  if (RELEASE_ASSETS.includes(name)) {
    return name;
  }
  if (!name.startsWith('aidget_') && !name.startsWith('aidget-')) {
    throw new Error(`unsupported release asset ${name}`);
  }

  if (name.endsWith('.dmg')) {
    if (/_(aarch64|arm64)\.dmg$/.test(name)) {
      return 'aidget-darwin-arm64.dmg';
    }
    if (/_(x64|x86_64|amd64)\.dmg$/.test(name)) {
      return 'aidget-darwin-amd64.dmg';
    }
  }
  if (/_(x64|x86_64|amd64)-setup\.exe$/.test(name)) {
    return 'aidget-windows-amd64.exe';
  }
  if (/_(aarch64|arm64)-setup\.exe$/.test(name)) {
    return 'aidget-windows-arm64.exe';
  }
  if (/_(x86_64|amd64)\.deb$/.test(name)) {
    return 'aidget-linux-amd64.deb';
  }
  if (/_(aarch64|arm64)\.deb$/.test(name)) {
    return 'aidget-linux-arm64.deb';
  }
  if (/\.(x86_64|amd64)\.rpm$/.test(name)) {
    return 'aidget-linux-amd64.rpm';
  }
  if (/\.(aarch64|arm64)\.rpm$/.test(name)) {
    return 'aidget-linux-arm64.rpm';
  }

  throw new Error(`unsupported release asset ${name}`);
}

async function filesUnder(root) {
  const out = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      out.push(...(await filesUnder(path)));
    } else if (entry.isFile()) {
      out.push(path);
    }
  }
  return out;
}

/** Collect every platform package and write SHA-256 entries for their final names. */
export async function collectAssets({ input, output }) {
  await mkdir(output, { recursive: true });
  const copied = new Map();

  for (const source of await filesUnder(input)) {
    const name = assetName(source);
    if (copied.has(name)) {
      throw new Error(`two source files map to ${name}`);
    }
    copied.set(name, source);
  }

  const expected = RELEASE_ASSETS;
  for (const name of expected) {
    if (!copied.has(name)) {
      throw new Error(`missing release asset ${name}`);
    }
    await copyFile(copied.get(name), join(output, name));
  }
  if (copied.size !== expected.length) {
    throw new Error(`expected ${expected.length} release assets, got ${copied.size}`);
  }

  const sums = [];
  for (const name of expected) {
    const digest = createHash('sha256')
      .update(await readFile(join(output, name)))
      .digest('hex');
    sums.push(`${digest}  ${name}`);
  }
  await writeFile(join(output, 'SHA256SUMS'), `${sums.join('\n')}\n`);
  return expected;
}

async function main(argv) {
  const values = new Map();
  for (let i = 0; i < argv.length; i += 2) {
    values.set(argv[i], argv[i + 1]);
  }
  parseTag(values.get('--version'));
  const input = values.get('--input');
  const output = values.get('--output');
  if (!input || !output) {
    throw new Error('usage: assets.mjs --version vX.Y.Z --input DIR --output DIR');
  }

  const names = await collectAssets({ input, output });
  console.log(`collected ${names.length} assets and SHA256SUMS`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`release assets: ${error.message}`);
    process.exitCode = 1;
  });
}
