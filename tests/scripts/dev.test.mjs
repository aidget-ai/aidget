import { execFile, spawn } from 'node:child_process';
import { once } from 'node:events';
import { chmod, copyFile, mkdir, mkdtemp, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';

import { expect, test } from 'vite-plus/test';

import { devArgs, devEnvironment } from '../../scripts/dev.mjs';

test('only macOS uses a Cargo runner to launch a bundle', () => {
  const args = ['--release', '--', '--features', 'example', '--', 'ui'];
  expect(devArgs(args, 'linux')).toEqual(['exec', 'tauri', 'dev', ...args]);
  const mac = devArgs(args, 'darwin');
  expect(mac.slice(0, 5)).toEqual(['exec', 'tauri', 'dev', '--release', '--']);
  expect(mac[5]).toBe('--config');
  expect(mac[6]).toContain('runner');
  expect(mac.slice(7)).toEqual(['--features', 'example', '--', 'ui']);
});

test('development uses an isolated data directory and socket', () => {
  const root = resolve('/tmp/aidget');

  expect(
    devEnvironment(root, {
      PATH: '/usr/bin',
    }),
  ).toEqual({
    AIDGET_HOME: resolve(root, '.aidget'),
    PATH: '/usr/bin',
  });
});

test('development preserves an explicit data directory', () => {
  expect(devEnvironment('/tmp/aidget', { AIDGET_HOME: '/tmp/aidget-test' })).toEqual({
    AIDGET_HOME: '/tmp/aidget-test',
  });
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
      expect(forwarded.slice(-3)).toEqual(['--', 'ui', '--no-main-window']);
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
