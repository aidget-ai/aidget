import { spawn } from 'node:child_process';
import { chmod, copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** Forward Tauri, Cargo and app arguments while isolating development data. */
export function devArgs(args, platform = process.platform, directory = root) {
  const separator = args.indexOf('--');
  const tauriArgs = separator < 0 ? [...args] : args.slice(0, separator);
  const forwarded = separator < 0 ? [] : args.slice(separator + 1);
  const appSeparator = forwarded.indexOf('--');
  const cargoArgs = appSeparator < 0 ? forwarded : forwarded.slice(0, appSeparator);
  const appArgs = appSeparator < 0 ? [] : forwarded.slice(appSeparator + 1);
  const homeIndex = tauriArgs.findIndex((arg) => arg === '--home' || arg.startsWith('--home='));
  let home = resolve(directory, '.aidget');
  if (homeIndex >= 0) {
    const option = tauriArgs[homeIndex];
    home = option === '--home' ? tauriArgs[homeIndex + 1] : option.slice('--home='.length);
    if (!home || home.startsWith('--')) {
      throw new Error('--home requires a directory');
    }
    if (appArgs.some((arg) => arg === '--home' || arg.startsWith('--home='))) {
      throw new Error('pass --home either to dev or to the app, not both');
    }
    tauriArgs.splice(homeIndex, option === '--home' ? 2 : 1);
    home = resolve(home);
  }
  if (!appArgs.some((arg) => arg === '--home' || arg.startsWith('--home='))) {
    appArgs.push('--home', home);
  }
  const runnerArgs =
    platform === 'darwin'
      ? [
          '--config',
          `target.'cfg(target_os = "macos")'.runner = ${JSON.stringify([process.execPath, fileURLToPath(import.meta.url), '--run-app'])}`,
        ]
      : [];
  return [
    'exec',
    'tauri',
    'dev',
    ...tauriArgs,
    '--',
    ...runnerArgs,
    ...cargoArgs,
    '--',
    ...appArgs,
  ];
}

/** Invoke pnpm without a shell so Windows preserves paths containing spaces. */
export function pnpmCommand(args, platform = process.platform, entry = process.env.npm_execpath) {
  if (platform !== 'win32') {
    return ['pnpm', ...args];
  }
  if (!entry) {
    throw new Error('start development with pnpm dev');
  }
  return /\.(?:c|m)?js$/iu.test(entry) ? [process.execPath, entry, ...args] : [entry, ...args];
}

async function main() {
  const args = process.argv.slice(2);
  if (args[0] === '--run-app') {
    const binary = resolve(args[1]);
    const contents = resolve(dirname(binary), 'aidget-dev.app/Contents');
    const executable = resolve(contents, 'MacOS/aidget');
    await mkdir(dirname(executable), { recursive: true });
    await copyFile(binary, executable);
    await chmod(executable, 0o755);
    const plist = await readFile(resolve(root, 'Info.plist'), 'utf8');
    await writeFile(
      resolve(contents, 'Info.plist'),
      plist.replace(
        '<dict>',
        `<dict>
  <key>CFBundleIdentifier</key><string>dev.aidget.desktop.dev</string>
  <key>CFBundleExecutable</key><string>aidget</string>
  <key>CFBundleName</key><string>aidget dev</string>
  <key>CFBundlePackageType</key><string>APPL</string>`,
      ),
    );
    // Preserve Cargo's PID so Tauri terminates the application itself on reload.
    process.execve(executable, [executable, ...args.slice(2)], process.env);
    return;
  }
  const [program, ...commandArgs] = pnpmCommand(devArgs(args));
  const child = spawn(program, commandArgs, {
    cwd: root,
    stdio: 'inherit',
  });
  child.on('error', (error) => {
    console.error(`dev: ${error.message}`);
    process.exitCode = 1;
  });
  child.on('exit', (code) => {
    process.exitCode = code ?? 1;
  });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`dev: ${error.message}`);
    process.exitCode = 1;
  });
}
