import { describe, expect, it } from 'vite-plus/test';

import { formatPathForDisplay } from '../../src/utils/path';

describe('path display', () => {
  it.each([
    [
      String.raw`C:\Users\Micooz\.pi/agent\mcp.json`,
      String.raw`C:\Users\Micooz\.pi\agent\mcp.json`,
    ],
    ['c:/Users/用户/.pi/agent/skills/aidget', String.raw`c:\Users\用户\.pi\agent\skills\aidget`],
    [String.raw`\\server\share/.pi/agent\mcp.json`, String.raw`\\server\share\.pi\agent\mcp.json`],
    [String.raw`\\?\C:\Users\Micooz/.pi/agent`, String.raw`\\?\C:\Users\Micooz\.pi\agent`],
    [
      String.raw`C:\Users\Micooz\.codex\config.toml`,
      String.raw`C:\Users\Micooz\.codex\config.toml`,
    ],
  ])('formats %s as %s', (path, expected) => {
    expect(formatPathForDisplay(path)).toBe(expected);
  });

  it.each(['/Users/micooz/.pi/agent/mcp.json', String.raw`/home/user/back\slash/mcp.json`])(
    'preserves POSIX path %s',
    (path) => {
      expect(formatPathForDisplay(path)).toBe(path);
    },
  );
});
