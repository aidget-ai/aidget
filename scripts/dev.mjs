import { spawn } from 'node:child_process';
import { chmod, copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** Use Cargo's executable runner on macOS without changing its build or target options. */
export function devArgs(args, platform = process.platform) {
  if (platform !== 'darwin') {
    return ['exec', 'tauri', 'dev', ...args];
  }
  const separator = args.indexOf('--');
  const tauriArgs = separator < 0 ? args : args.slice(0, separator);
  const cargoArgs = separator < 0 ? [] : args.slice(separator + 1);
  const runner = [process.execPath, fileURLToPath(import.meta.url), '--run-app'];
  return [
    'exec',
    'tauri',
    'dev',
    ...tauriArgs,
    '--',
    '--config',
    `target.'cfg(target_os = "macos")'.runner = ${JSON.stringify(runner)}`,
    ...cargoArgs,
  ];
}

/** Keep development state and IPC separate from an installed aidget instance. */
export function devEnvironment(root, environment = process.env) {
  return {
    ...environment,
    AIDGET_HOME: environment.AIDGET_HOME ?? resolve(root, '.aidget'),
  };
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
  const child = spawn('pnpm', devArgs(args), {
    cwd: root,
    env: devEnvironment(root),
    shell: process.platform === 'win32',
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
