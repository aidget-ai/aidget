import { execFile, spawn } from 'node:child_process';
import { once } from 'node:events';
import { chmod, copyFile, mkdir, mkdtemp, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';

import { expect, test } from 'vite-plus/test';

import { devArgs, pnpmCommand } from '../../scripts/dev.mjs';

test('only macOS uses a Cargo runner to launch a bundle', () => {
  const args = ['--release', '--', '--features', 'example', '--', 'ui'];
  expect(devArgs(args, 'linux', '/tmp/repo')).toEqual([
    'exec',
    'tauri',
    'dev',
    '--release',
    '--',
    '--features',
    'example',
    '--',
    'ui',
    '--home',
    resolve('/tmp/repo/.aidget'),
  ]);
  const mac = devArgs(args, 'darwin', '/tmp/repo');
  expect(mac.slice(0, 5)).toEqual(['exec', 'tauri', 'dev', '--release', '--']);
  expect(mac[5]).toBe('--config');
  expect(mac[6]).toContain('runner');
  expect(mac.slice(7)).toEqual([
    '--features',
    'example',
    '--',
    'ui',
    '--home',
    resolve('/tmp/repo/.aidget'),
  ]);
});

test('development accepts an explicit data directory', () => {
  for (const platform of ['linux', 'win32', 'darwin']) {
    expect(devArgs(['--home', '/tmp/custom home'], platform).slice(-3)).toEqual([
      '--',
      '--home',
      resolve('/tmp/custom home'),
    ]);
    expect(devArgs(['--home=/tmp/custom home'], platform).slice(-2)).toEqual([
      '--home',
      resolve('/tmp/custom home'),
    ]);
    expect(devArgs(['--', '--', 'ui', '--home', '/tmp/app home'], platform).slice(-3)).toEqual([
      'ui',
      '--home',
      '/tmp/app home',
    ]);
  }
});

test('development rejects a missing or conflicting home argument', () => {
  expect(() => devArgs(['--home'])).toThrow('--home requires a directory');
  expect(() => devArgs(['--home='])).toThrow('--home requires a directory');
  expect(() => devArgs(['--home', '/tmp/one', '--', '--', '--home', '/tmp/two'])).toThrow(
    'not both',
  );
});

test.skipIf(process.platform === 'win32')(
  'development forwards CLI and application arguments',
  async () => {
    const root = await realpath(await mkdtemp(join(tmpdir(), 'aidget-dev-entry-')));
    try {
      await mkdir(join(root, 'scripts'));
      await copyFile(resolve('scripts/dev.mjs'), join(root, 'scripts/dev.mjs'));
      const pnpm = join(root, 'pnpm');
      await writeFile(
        pnpm,
        `#!${process.execPath}\nconsole.log(JSON.stringify(process.argv.slice(2)));\n`,
      );
      await chmod(pnpm, 0o755);
      const args = ['--no-watch', '--', '--', 'ui', '--no-main-window'];
      const { stdout } = await promisify(execFile)(
        process.execPath,
        [join(root, 'scripts/dev.mjs'), ...args],
        { env: { ...process.env, PATH: `${root}:${process.env.PATH}` } },
      );
      const forwarded = JSON.parse(stdout);
      expect(forwarded.slice(0, 4)).toEqual(['exec', 'tauri', 'dev', '--no-watch']);
      expect(forwarded.slice(-5)).toEqual([
        '--',
        'ui',
        '--no-main-window',
        '--home',
        join(root, '.aidget'),
      ]);
      if (process.platform === 'darwin') {
        expect(forwarded[6]).toContain(join(root, 'scripts/dev.mjs'));
      }
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  },
);

test.skipIf(process.platform !== 'darwin')(
  'bundled launch keeps the Cargo runner PID and application arguments',
  async () => {
    const root = await realpath(await mkdtemp(join(tmpdir(), 'aidget dev bundle ')));
    let runner;
    let applicationPid;
    try {
      await mkdir(join(root, 'scripts'));
      await copyFile(resolve('scripts/dev.mjs'), join(root, 'scripts/dev.mjs'));
      await copyFile(resolve('Info.plist'), join(root, 'Info.plist'));
      const binary = join(root, 'aidget');
      await writeFile(
        binary,
        `#!${process.execPath}\nconsole.log(JSON.stringify({pid: process.pid, args: process.argv.slice(2), path: process.argv[1]}));\nsetInterval(() => {}, 1000);\n`,
      );
      await chmod(binary, 0o755);
      runner = spawn(
        process.execPath,
        [join(root, 'scripts/dev.mjs'), '--run-app', binary, 'ui', '--no-main-window'],
        { stdio: ['ignore', 'pipe', 'inherit'] },
      );
      const [output] = await once(runner.stdout, 'data', { signal: AbortSignal.timeout(4000) });
      const application = JSON.parse(output.toString());
      applicationPid = application.pid;
      expect(application.args).toEqual(['ui', '--no-main-window']);
      expect(application.path).toBe(join(root, 'aidget-dev.app/Contents/MacOS/aidget'));
      expect(applicationPid).toBe(runner.pid);
      const exited = once(runner, 'exit');
      runner.kill('SIGKILL');
      await exited;
      expect(() => process.kill(applicationPid, 0)).toThrow();
    } finally {
      if (runner && runner.exitCode === null && runner.signalCode === null) {
        const exited = once(runner, 'exit');
        runner.kill('SIGKILL');
        await exited;
      }
      if (applicationPid && applicationPid !== runner?.pid) {
        process.kill(applicationPid, 'SIGKILL');
      }
      await rm(root, { recursive: true, force: true });
    }
  },
);

test('development passes its data directory as an application argument', () => {
  for (const platform of ['linux', 'win32', 'darwin']) {
    expect(devArgs([], platform, '/tmp/aidget-repo').slice(-3)).toEqual([
      '--',
      '--home',
      resolve('/tmp/aidget-repo/.aidget'),
    ]);
  }
});

test('Windows launches pnpm directly and preserves path arguments', () => {
  const args = ['exec', 'tauri', 'dev', '--', '--', '--home', 'C:\\data space'];
  expect(pnpmCommand(args, 'win32', 'C:\\pnpm space\\pnpm.exe')).toEqual([
    'C:\\pnpm space\\pnpm.exe',
    ...args,
  ]);
  expect(pnpmCommand(args, 'win32', 'C:\\pnpm space\\pnpm.cjs')).toEqual([
    process.execPath,
    'C:\\pnpm space\\pnpm.cjs',
    ...args,
  ]);
  expect(() => pnpmCommand(args, 'win32', '')).toThrow('pnpm dev');
});
