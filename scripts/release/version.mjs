import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const TAG_PATTERN = /^v((?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-beta\.(?:0|[1-9]\d*))?)$/;

function parsePackageVersion(toml) {
  let inPackage = false;
  let version = null;

  for (const line of toml.split('\n')) {
    const trimmed = line.trim();
    if (/^\[[^\]]+\]$/.test(trimmed)) {
      inPackage = trimmed === '[package]';
      continue;
    }
    if (!inPackage || version !== null || !/^version\s*=/.test(trimmed)) {
      continue;
    }
    const match = trimmed.match(/^version\s*=\s*"([^"]+)"/);
    if (!match) {
      throw new Error('Cargo.toml package version is not a TOML string');
    }
    version = match[1];
  }

  if (version === null) {
    throw new Error('Cargo.toml has no version in its [package] section');
  }
  return version;
}

function replacePackageVersion(toml, version) {
  const lines = toml.split('\n');
  let inPackage = false;
  let replaced = false;

  for (let i = 0; i < lines.length; i += 1) {
    const trimmed = lines[i].trim();
    if (/^\[[^\]]+\]$/.test(trimmed)) {
      inPackage = trimmed === '[package]';
      continue;
    }
    if (inPackage && !replaced && /^version\s*=/.test(trimmed)) {
      lines[i] = `version = "${version}"`;
      replaced = true;
    }
  }

  if (!replaced) {
    throw new Error('Cargo.toml has no version to replace in its [package] section');
  }
  return lines.join('\n');
}

/** Parse a stable or numbered beta release tag into its manifest version. */
export function parseTag(tag) {
  const match = String(tag).match(TAG_PATTERN);
  if (!match) {
    throw new Error(
      `release tag must be vMAJOR.MINOR.PATCH or vMAJOR.MINOR.PATCH-beta.N, got ${JSON.stringify(tag)}`,
    );
  }
  return match[1];
}

/** Whether a valid release tag is a beta that must not become Latest. */
export function isPrereleaseTag(tag) {
  return parseTag(tag).includes('-beta.');
}

/** Compare supported release tags using SemVer precedence. */
export function isNewerTag(candidate, current) {
  const [nextCore, nextBeta] = parseTag(candidate).split('-beta.');
  const [latestCore, latestBeta] = parseTag(current).split('-beta.');
  const next = nextCore.split('.').map(BigInt);
  const latest = latestCore.split('.').map(BigInt);

  for (let i = 0; i < 3; i += 1) {
    if (next[i] !== latest[i]) {
      return next[i] > latest[i];
    }
  }
  // A stable release sorts after every beta of the same core version.
  if (nextBeta === undefined) {
    return latestBeta !== undefined;
  }
  return latestBeta !== undefined && BigInt(nextBeta) > BigInt(latestBeta);
}

async function readJson(path, label) {
  const value = JSON.parse(await readFile(path, 'utf8'));
  if (typeof value.version !== 'string') {
    throw new Error(`${label} has no string version`);
  }
  return value;
}

export async function readVersions(root) {
  const cargo = await readFile(join(root, 'Cargo.toml'), 'utf8');
  const packageJson = await readJson(join(root, 'package.json'), 'package.json');
  const tauriConfig = await readJson(join(root, 'tauri.conf.json'), 'tauri.conf.json');

  return {
    cargo: parsePackageVersion(cargo),
    packageJson: packageJson.version,
    tauriConfig: tauriConfig.version,
  };
}

export async function setVersion(root, tag) {
  const version = parseTag(tag);
  const cargoPath = join(root, 'Cargo.toml');
  const packagePath = join(root, 'package.json');
  const tauriPath = join(root, 'tauri.conf.json');

  const cargo = await readFile(cargoPath, 'utf8');
  const packageJson = await readJson(packagePath, 'package.json');
  const tauriConfig = await readJson(tauriPath, 'tauri.conf.json');

  await writeFile(cargoPath, replacePackageVersion(cargo, version));
  await writeFile(packagePath, `${JSON.stringify({ ...packageJson, version }, null, 2)}\n`);
  await writeFile(tauriPath, `${JSON.stringify({ ...tauriConfig, version }, null, 2)}\n`);

  return version;
}

export async function checkVersion(root, tag) {
  const expected = parseTag(tag);
  const versions = await readVersions(root);
  for (const [name, actual] of Object.entries(versions)) {
    if (actual !== expected) {
      throw new Error(`${name} version is ${actual}, expected ${expected}`);
    }
  }
  return expected;
}

async function main(argv) {
  const [command, first, second] = argv;
  const root = process.cwd();

  switch (command) {
    case 'parse':
      console.log(parseTag(first));
      return;
    case 'prerelease':
      console.log(isPrereleaseTag(first));
      return;
    case 'set':
      console.log(await setVersion(root, first));
      return;
    case 'check':
      console.log(await checkVersion(root, first));
      return;
    case 'newer':
      if (!isNewerTag(first, second)) {
        throw new Error(`${first} is not newer than ${second}`);
      }
      return;
    default:
      throw new Error('usage: version.mjs <parse|prerelease|set|check|newer> <tag> [previous-tag]');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`release version: ${error.message}`);
    process.exitCode = 1;
  });
}
