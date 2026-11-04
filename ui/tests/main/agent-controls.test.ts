/// @vitest-environment happy-dom

import { describe, expect, it, vi } from 'vite-plus/test';

import { renderAgentStrip } from '../../src/main/agent-controls';
import type { IntegrationInfo } from '../../src/main/integration';
import { translate } from '../../src/main/locale';

const info: IntegrationInfo = {
  mcpCommand: 'aidget',
  mcpDisplayCommand: 'aidget',
  mcpArgs: ['mcp'],
  mcpDisplayArgs: ['mcp'],
  skillDirectory: '/tmp/skills',
  dataDirectory: '/tmp/home',
  skillContent: '',
  agents: [
    {
      id: 'codex',
      name: 'Codex',
      icon: 'codex-color',
      mcp: { path: '/tmp/codex.toml', installed: false, replaceable: false },
    },
    {
      id: 'cursor',
      name: 'Cursor',
      icon: 'cursor',
      mcp: { path: '/tmp/cursor.json', installed: false, replaceable: false },
    },
  ],
};

describe('Agent controls', () => {
  it.each(['mcp', 'skills'] as const)(
    'uses consistent Windows separators in %s tooltips and accessible descriptions',
    (resource) => {
      const path =
        resource === 'mcp'
          ? String.raw`C:\Users\Micooz\.pi/agent\mcp.json`
          : String.raw`C:\Users\Micooz\.pi/agent\skills\aidget`;
      const displayPath =
        resource === 'mcp'
          ? String.raw`C:\Users\Micooz\.pi\agent\mcp.json`
          : String.raw`C:\Users\Micooz\.pi\agent\skills\aidget`;
      for (const [installed, replaceable, action] of [
        [false, false, '为 Pi 安装：'],
        [false, true, '为 Pi 覆盖安装：'],
        [true, false, '从 Pi 移除：'],
      ] as const) {
        const state = { path, installed, replaceable };
        const root = document.createElement('div');
        const onChange = vi.fn();
        renderAgentStrip(
          root,
          {
            ...info,
            agents: [{ id: 'pi', name: 'Pi', icon: 'pi', [resource]: state }],
          },
          resource,
          {
            all: '全部',
            empty: '无 Agent',
            installFor: translate('zh', 'installFor'),
            replaceFor: translate('zh', 'replaceFor'),
            removeFrom: translate('zh', 'removeFrom'),
          },
          false,
          onChange,
        );
        const button = root.querySelector<HTMLButtonElement>('[data-agent="pi"]')!;
        expect(button.title).toBe(`${action}${displayPath}`);
        expect(button.getAttribute('aria-description')).toBe(displayPath);
        button.click();
        expect(onChange).toHaveBeenCalledWith(['pi'], !installed);
        expect(state.path).toBe(path);
      }
    },
  );

  it('names the Agent in both removal tooltips and accessible labels', () => {
    const installedInfo: IntegrationInfo = {
      ...info,
      agents: [
        {
          ...info.agents[0],
          mcp: { path: '/tmp/config.toml', installed: true, replaceable: false },
          skills: { path: '/tmp/agent/skills/aidget', installed: true, replaceable: false },
        },
      ],
    };
    for (const locale of ['zh', 'en'] as const) {
      for (const resource of ['mcp', 'skills'] as const) {
        const root = document.createElement('div');
        const onChange = vi.fn();
        renderAgentStrip(
          root,
          installedInfo,
          resource,
          {
            all: translate(locale, 'agentAll'),
            empty: translate(locale, 'integrationNoAgents'),
            installFor: translate(locale, 'installFor'),
            replaceFor: translate(locale, 'replaceFor'),
            removeFrom: translate(locale, 'removeFrom'),
          },
          false,
          onChange,
        );
        const button = root.querySelector<HTMLButtonElement>('[data-agent="codex"]')!;
        const action = locale === 'zh' ? '从 Codex 移除：' : 'Remove from Codex: ';
        expect(button.getAttribute('aria-label')).toBe(action);
        expect(button.title).toBe(`${action}${installedInfo.agents[0][resource]!.path}`);
        button.click();
        expect(onChange).toHaveBeenCalledWith(['codex'], false);
      }
    }
  });

  it('renders color logos as images and monochrome logos as masks', () => {
    const root = document.createElement('div');

    renderAgentStrip(
      root,
      info,
      'mcp',
      {
        all: 'All',
        empty: 'No Agents',
        installFor: 'Install for',
        replaceFor: 'Replace for',
        removeFrom: 'Remove from {agent}: ',
      },
      false,
      vi.fn(),
    );

    const codex = root.querySelector<HTMLElement>('[data-agent="codex"]')!;
    expect(codex.querySelector('img')).not.toBeNull();
    expect(codex.querySelector('.agent-icon-mask')).toBeNull();

    const cursor = root.querySelector<HTMLElement>('[data-agent="cursor"]')!;
    expect(cursor.querySelector('img')).toBeNull();
    expect(
      cursor
        .querySelector<HTMLElement>('.agent-icon-mask')!
        .style.getPropertyValue('--agent-icon-url'),
    ).not.toBe('');
  });

  it('labels both install buttons for the specific Agent', () => {
    const installInfo: IntegrationInfo = {
      ...info,
      agents: [
        {
          id: 'gemini',
          name: 'Gemini CLI',
          icon: 'gemini',
          mcp: { path: '/tmp/gemini/mcp.json', installed: false, replaceable: false },
          skills: { path: '/tmp/gemini/skills', installed: false, replaceable: false },
        },
      ],
    };
    const labels = {
      all: 'All',
      empty: 'No Agents',
      installFor: '为 {agent} 安装：',
      replaceFor: '为 {agent} 覆盖安装：',
      removeFrom: 'Remove from {agent}: ',
    };

    for (const [resource, path] of [
      ['mcp', '/tmp/gemini/mcp.json'],
      ['skills', '/tmp/gemini/skills'],
    ] as const) {
      const root = document.createElement('div');
      renderAgentStrip(root, installInfo, resource, labels, false, vi.fn());

      expect(root.querySelector('[data-agent="gemini"]')?.getAttribute('title')).toBe(
        `为 Gemini CLI 安装：${path}`,
      );
    }
  });
});

describe('integration switching', () => {
  const labels = {
    all: 'All',
    empty: 'No Agents',
    installFor: 'Install for {agent}: ',
    replaceFor: 'Replace for {agent}: ',
    removeFrom: 'Remove from {agent}: ',
  };

  it('offers replacement for another instance and includes it in All', () => {
    const root = document.createElement('div');
    const onChange = vi.fn();
    const state: IntegrationInfo = {
      ...info,
      agents: [
        { ...info.agents[0], mcp: { path: '/tmp/config', installed: false, replaceable: true } },
      ],
    };
    renderAgentStrip(root, state, 'mcp', labels, false, onChange);
    const button = root.querySelector<HTMLButtonElement>('[data-agent="codex"]')!;
    expect(button.disabled).toBe(false);
    expect(button.getAttribute('aria-label')).toBe('Replace for Codex: ');
    expect(button.getAttribute('aria-description')).toBe('/tmp/config');
    button.click();
    expect(onChange).toHaveBeenLastCalledWith(['codex'], true);
    root.querySelector<HTMLButtonElement>('.agent-chip-all')!.click();
    expect(onChange).toHaveBeenLastCalledWith(['codex'], true);
  });

  it('blocks unreadable configurations and exposes the reason', () => {
    const root = document.createElement('div');
    const onChange = vi.fn();
    const state: IntegrationInfo = {
      ...info,
      agents: [
        {
          ...info.agents[0],
          mcp: {
            path: '/tmp/config',
            installed: false,
            replaceable: false,
            conflict: 'Invalid TOML',
          },
        },
        info.agents[1],
      ],
    };
    renderAgentStrip(root, state, 'mcp', labels, false, onChange);
    const button = root.querySelector<HTMLButtonElement>('[data-agent="codex"]')!;
    expect(button.disabled).toBe(true);
    expect(button.getAttribute('aria-description')).toContain('Invalid TOML');
    button.click();
    expect(onChange).not.toHaveBeenCalled();
    root.querySelector<HTMLButtonElement>('.agent-chip-all')!.click();
    expect(onChange).toHaveBeenCalledWith(['cursor'], true);
  });
});
