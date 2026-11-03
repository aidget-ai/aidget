import { describe, expect, it } from 'vite-plus/test';

import {
  allInstalled,
  integrationAgents,
  mcpCommandLine,
  type IntegrationInfo,
} from '../../src/main/integration';

const info: IntegrationInfo = {
  mcpCommand: '/Applications/aidget.app/Contents/MacOS/aidget',
  mcpDisplayCommand: '/Applications/aidget.app/Contents/MacOS/aidget',
  mcpArgs: ['mcp'],
  mcpDisplayArgs: ['mcp'],
  skillDirectory: 'skills/aidget',
  dataDirectory: '/tmp/home',
  skillContent: '---\nname: aidget\n---\n',
  agents: [
    {
      id: 'codex',
      name: 'Codex',
      icon: 'codex-color',
      mcp: { path: '/tmp/config.toml', installed: true, replaceable: false },
      skills: { path: '/tmp/skills', installed: false, replaceable: false },
    },
    {
      id: 'zed',
      name: 'Zed',
      icon: 'zed',
      mcp: { path: '/tmp/settings.json', installed: false, replaceable: false },
    },
  ],
};

describe('integration configuration', () => {
  it('hides the default home only in the displayed command', () => {
    const configured = {
      mcpCommand: '/Users/test/.aidget/aidget',
      mcpDisplayCommand: '/Users/test/.aidget/aidget',
      mcpArgs: ['--home', '/Users/test/.aidget', 'mcp'],
      mcpDisplayArgs: ['mcp'],
    };
    expect(mcpCommandLine(configured)).toBe('/Users/test/.aidget/aidget mcp');
    expect(configured.mcpArgs).toEqual(['--home', '/Users/test/.aidget', 'mcp']);
  });

  it('renders the complete MCP command', () => {
    expect(mcpCommandLine(info)).toBe('/Applications/aidget.app/Contents/MacOS/aidget mcp');
    expect(
      mcpCommandLine({
        mcpDisplayCommand: '/Applications/Aidget Beta.app/Contents/MacOS/aidget',
        mcpDisplayArgs: ['mcp', '--profile', 'local dev'],
      }),
    ).toBe('"/Applications/Aidget Beta.app/Contents/MacOS/aidget" mcp --profile "local dev"');
  });

  it('shows the Windows executable with its custom home', () => {
    const home = String.raw`E:\Projects\aidget\.aidget`;
    const executable = `${home}\\aidget.exe`;
    const configured = {
      mcpCommand: executable,
      mcpArgs: ['--home', home, 'mcp'],
      mcpDisplayCommand: executable,
      mcpDisplayArgs: ['--home', home, 'mcp'],
    };
    expect(mcpCommandLine(configured)).toBe(`${executable} --home ${home} mcp`);
  });

  it('quotes Windows paths with spaces without doubling backslashes', () => {
    const home = String.raw`E:\My Projects\aidget\.aidget`;
    const executable = `${home}\\aidget.exe`;
    expect(
      mcpCommandLine({
        mcpDisplayCommand: executable,
        mcpDisplayArgs: ['--home', home, 'mcp'],
      }),
    ).toBe(`"${executable}" --home "${home}" mcp`);
  });

  it('omits the default Windows home from the displayed command', () => {
    const cli = String.raw`C:\Users\test\.aidget\aidget.exe`;
    expect(mcpCommandLine({ mcpDisplayCommand: cli, mcpDisplayArgs: ['mcp'] })).toBe(`${cli} mcp`);
  });

  it('shows only Agents that can receive the current resource', () => {
    expect(integrationAgents(info, 'mcp').map((agent) => agent.id)).toEqual(['codex', 'zed']);
    expect(integrationAgents(info, 'skills').map((agent) => agent.id)).toEqual(['codex']);
    expect(allInstalled(info, 'mcp')).toBe(false);
    expect(
      allInstalled(
        {
          ...info,
          agents: [
            {
              ...info.agents[0],
              mcp: { path: '/tmp/config.toml', installed: true, replaceable: false },
            },
          ],
        },
        'mcp',
      ),
    ).toBe(true);
  });
});
