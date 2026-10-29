import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { parseTag } from './version.mjs';

export function assetName(source, version) {
  const name = basename(source);

  if (name.endsWith('.dmg')) {
    if (/aarch64|arm64/i.test(name)) {
      return `aidget_${version}_aarch64.dmg`;
    }
    if (/x64|x86_64|amd64/i.test(name)) {
      return `aidget_${version}_x64.dmg`;
    }
  }
  if (name.endsWith('.app.zip')) {
    // The updater swaps this archive in, so it must keep the `.app` wrapper:
    // pack it with `ditto -c -k --keepParent`.
    if (/aarch64|arm64/i.test(name)) {
      return `aidget_${version}_aarch64.app.zip`;
    }
    if (/x64|x86_64|amd64/i.test(name)) {
      return `aidget_${version}_x64.app.zip`;
    }
  }
  if (name.endsWith('.exe')) {
    return `aidget_${version}_x64-setup.exe`;
  }
  if (name.endsWith('.AppImage')) {
    return `aidget_${version}_amd64.AppImage`;
  }
  if (name.endsWith('.deb')) {
    return `aidget_${version}_amd64.deb`;
  }
  if (name.endsWith('.rpm')) {
    return `aidget_${version}_amd64.rpm`;
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

export async function collectAssets({ input, output, version }) {
  await mkdir(output, { recursive: true });
  const copied = new Map();

  for (const source of await filesUnder(input)) {
    const name = assetName(source, version);
    if (copied.has(name)) {
      throw new Error(`two source files map to ${name}`);
    }
    copied.set(name, source);
  }

  const expected = [
    `aidget_${version}_aarch64.dmg`,
    `aidget_${version}_x64.dmg`,
    `aidget_${version}_aarch64.app.zip`,
    `aidget_${version}_x64.app.zip`,
    `aidget_${version}_x64-setup.exe`,
    `aidget_${version}_amd64.AppImage`,
    `aidget_${version}_amd64.deb`,
    `aidget_${version}_amd64.rpm`,
  ];
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
  const version = parseTag(values.get('--version'));
  const input = values.get('--input');
  const output = values.get('--output');
  if (!input || !output) {
    throw new Error('usage: assets.mjs --version vX.Y.Z --input DIR --output DIR');
  }

  const names = await collectAssets({ input, output, version });
  console.log(`collected ${names.length} assets and SHA256SUMS`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`release assets: ${error.message}`);
    process.exitCode = 1;
  });
}
