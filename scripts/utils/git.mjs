import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

/** Run Git without a shell in an optional working directory and return UTF-8 output. */
export async function runGit(args, cwd) {
  const { stdout } = await execFileAsync('git', args, {
    encoding: 'utf8',
    maxBuffer: 10 * 1024 * 1024,
    cwd,
  });
  return stdout;
}
