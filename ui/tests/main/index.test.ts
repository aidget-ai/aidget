/// @vitest-environment happy-dom

import { readFileSync } from 'node:fs';

import type { EventCallback } from '@tauri-apps/api/event';
import { beforeEach, describe, expect, it, vi } from 'vite-plus/test';

import type { IntegrationInfo } from '../../src/main/integration';
import type { MainState } from '../../src/main/model';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(async (_event: string, _handler: EventCallback<MainState>) => () => {}),
  openUrl: vi.fn(async () => {}),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: mocks.openUrl }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: vi.fn(async () => {}),
    minimize: vi.fn(async () => {}),
    startDragging: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
  }),
}));
vi.mock('@tauri-apps/plugin-autostart', () => ({
  disable: vi.fn(async () => {}),
  enable: vi.fn(async () => {}),
  isEnabled: vi.fn(async () => false),
}));
vi.mock('lucide', () => {
  const icon = {};
  return {
    Archive: icon,
    ArrowUpDown: icon,
    ArrowDown: icon,
    ArrowUp: icon,
    RefreshCw: icon,
    ExternalLink: icon,
    Info: icon,
    LayoutGrid: icon,
    Minus: icon,
    RotateCcw: icon,
    ServerCog: icon,
    Settings: icon,
    Square: icon,
    Trash: icon,
    UsersRound: icon,
    X: icon,
    createElement: () => document.createElement('svg'),
    createIcons: vi.fn(),
  };
});

const mainHtml = readFileSync('ui/main.html', 'utf8');

function state(lastPage: MainState['lastPage']): MainState {
  return {
    widgets: [],
    deletedWidgets: [],
    broken: [],
    version: '0.1.0',
    theme: 'system',
    language: 'system',
    trayClickAction: 'window',
    lastPage,
    githubToken: null,
    autoDownload: true,
  };
}

function integrationState(installed: boolean): IntegrationInfo {
  return {
    mcpCommand: '/Applications/aidget.app/Contents/MacOS/aidget',
    mcpArgs: ['mcp'],
    skillDirectory: '/Users/test/.aidget/skills/aidget',
    skillContent: '',
    agents: [
      {
        id: 'codex',
        name: 'Codex',
        icon: 'codex-color',
        mcp: { path: '/Users/test/.codex/config.toml', installed },
      },
    ],
  };
}

function prepareDocument() {
  const bodyStart = mainHtml.indexOf('<body>') + '<body>'.length;
  const bodyEnd = mainHtml.lastIndexOf('</body>');
  document.body.innerHTML = mainHtml
    .slice(bodyStart, bodyEnd)
    .replace('<script type="module" src="./src/main/index.ts"></script>', '');
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
    },
  );
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => {
    callback(0);
    return 1;
  });
}

async function loadMain(initial: MainState) {
  mocks.invoke.mockImplementation((command: string, args?: { preferences?: unknown }) => {
    if (command === 'main_state') {
      return Promise.resolve(initial);
    }
    if (command === 'main_painted') {
      return Promise.resolve(true);
    }
    if (command === 'set_preferences') {
      return Promise.resolve();
    }
    return Promise.resolve(args);
  });
  await import('../../src/main');
  await vi.waitFor(() => {
    expect(document.querySelector(`[data-page="${initial.lastPage}"]`)?.classList).toContain(
      'is-active',
    );
  });
}

function pageButton(page: string) {
  return document.querySelector<HTMLButtonElement>(`[data-page="${page}"]`)!;
}

beforeEach(() => {
  vi.resetModules();
  mocks.invoke.mockReset();
  mocks.listen.mockClear();
  mocks.openUrl.mockReset().mockResolvedValue(undefined);
  prepareDocument();
});

describe('main window navigation', () => {
  it('places the archive action before the enabled switch on widget cards', async () => {
    const initial = state('local-widgets');
    initial.widgets = [
      {
        id: 'clock',
        title: 'Clock',
        packageName: null,
        revision: 1,
        token: 'token',
        width: 360,
        height: 220,
        enabled: true,
        createdAt: 1,
      },
    ];

    await loadMain(initial);
    const actions = await vi.waitFor(() => {
      const value = document.querySelector<HTMLElement>('[data-widget="clock"] .widget-actions');
      expect(value).toBeTruthy();
      return value!;
    });

    expect([...actions.querySelectorAll('button')].map((button) => button.className)).toEqual([
      'widget-icon-button archive-button',
      'switch',
    ]);
  });

  it('ignores stale preview failures while the widget is toggled', async () => {
    const initial = state('local-widgets');
    initial.widgets = [
      {
        id: 'ace-of-hearts',
        title: 'Ace of Hearts',
        packageName: null,
        revision: 1,
        token: 'token',
        width: 220,
        height: 330,
        enabled: true,
        createdAt: 1,
      },
    ];
    const rejectPreviews: ((reason?: unknown) => void)[] = [];
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'main_state') {
        return Promise.resolve(initial);
      }
      if (command === 'main_painted') {
        return Promise.resolve(false);
      }
      if (command === 'main_widget_cached_preview') {
        return Promise.resolve(null);
      }
      if (command === 'main_widget_preview') {
        return new Promise((_resolve, reject) => {
          rejectPreviews.push(reject);
        });
      }
      return Promise.resolve();
    });

    await import('../../src/main');
    await vi.waitFor(() => expect(rejectPreviews).toHaveLength(1));
    const disabled = structuredClone(initial);
    disabled.widgets[0].enabled = false;
    const onState = mocks.listen.mock.calls.find(([name]) => name === 'aidget://main')![1];
    onState({ event: 'aidget://main', id: 0, payload: disabled });
    onState({ event: 'aidget://main', id: 0, payload: initial });
    await vi.waitFor(() => expect(rejectPreviews).toHaveLength(2));
    rejectPreviews[0](new Error('widget not found: ace-of-hearts'));
    await new Promise((resolve) => setTimeout(resolve, 0));

    const fault = document.querySelector<HTMLElement>('[data-fault]')!;
    expect(fault.hidden).toBe(true);
    expect(fault.textContent).not.toContain('widget not found');
  });

  it('saves a GitHub token once and only renders its prefix and suffix', async () => {
    const initial = state('general');
    let saved: string | null | undefined;
    mocks.invoke.mockImplementation((command: string, args?: { token?: string | null }) => {
      if (command === 'main_state') {
        return Promise.resolve(initial);
      }
      if (command === 'main_painted') {
        return Promise.resolve(true);
      }
      if (command === 'set_github_token') {
        saved = args?.token;
        return Promise.resolve({ masked: 'ghp_…WXYZ' });
      }
      return Promise.resolve();
    });
    await import('../../src/main');
    await vi.waitFor(() => {
      expect(document.querySelector('[data-github-token-form]')).toBeTruthy();
    });

    const form = document.querySelector<HTMLFormElement>('[data-github-token-form]')!;
    const input = form.querySelector<HTMLInputElement>('input')!;
    input.value = 'samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ';
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));

    await vi.waitFor(() => {
      expect(document.querySelector('[data-github-token-masked]')?.textContent).toBe('ghp_…WXYZ');
    });
    expect(saved).toBe('samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ');
    expect(form.hidden).toBe(true);
    expect(input.value).toBe('');
    expect(document.body.textContent).not.toContain('samplevalueABCDEFGHIJKLMNOPQRSTUVWXYZ');
  });

  it('keeps a failed GitHub token input available for correction', async () => {
    const initial = state('general');
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'main_state') {
        return Promise.resolve(initial);
      }
      if (command === 'main_painted') {
        return Promise.resolve(true);
      }
      if (command === 'set_github_token') {
        return Promise.reject(new Error('GET https://api.github.com/repos: HTTP 401 Unauthorized'));
      }
      return Promise.resolve();
    });
    await import('../../src/main');
    const form = await vi.waitFor(() => {
      const value = document.querySelector<HTMLFormElement>('[data-github-token-form]');
      expect(value).toBeTruthy();
      return value!;
    });
    const input = form.querySelector<HTMLInputElement>('input')!;
    input.value = 'ghp_invalid1234567890abcdefghijkl';
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));

    await vi.waitFor(() => {
      expect(document.querySelector('[data-github-token-error]')?.textContent).toContain(
        'HTTP 401',
      );
    });
    expect(form.hidden).toBe(false);
    expect(input.value).toBe('ghp_invalid1234567890abcdefghijkl');
  });

  it('requires removal before a configured GitHub token can be replaced', async () => {
    const initial = state('general');
    initial.githubToken = { masked: 'ghp_…WXYZ' };
    let removed: string | null | undefined;
    mocks.invoke.mockImplementation((command: string, args?: { token?: string | null }) => {
      if (command === 'main_state') {
        return Promise.resolve(initial);
      }
      if (command === 'main_painted') {
        return Promise.resolve(true);
      }
      if (command === 'set_github_token') {
        removed = args?.token;
        return Promise.resolve(null);
      }
      return Promise.resolve();
    });
    await import('../../src/main');
    await vi.waitFor(() => {
      expect(document.querySelector('[data-github-token-masked]')?.textContent).toBe('ghp_…WXYZ');
    });
    expect(document.querySelector<HTMLFormElement>('[data-github-token-form]')!.hidden).toBe(true);

    document.querySelector<HTMLButtonElement>('[data-github-token-remove]')!.click();

    await vi.waitFor(() => {
      expect(document.querySelector<HTMLFormElement>('[data-github-token-form]')!.hidden).toBe(
        false,
      );
    });
    expect(removed).toBeNull();
    expect(document.activeElement).toBe(
      document.querySelector<HTMLInputElement>('[data-github-token-input]'),
    );
  });

  it('clears broken-widget errors after the configuration is repaired', async () => {
    const initial = state('about');
    initial.broken = [{ id: 'clock', error: 'invalid config' }];
    await loadMain(initial);
    const fault = document.querySelector<HTMLElement>('[data-fault]')!;
    expect(fault.hidden).toBe(false);
    expect(fault.textContent).toContain('clock: invalid config');

    const onState = mocks.listen.mock.calls.find(([name]) => name === 'aidget://main')![1];
    onState({ event: 'aidget://main', id: 0, payload: state('about') });
    expect(fault.hidden).toBe(true);
  });

  it('preserves an operation error while broken-widget state changes', async () => {
    const initial = state('about');
    initial.broken = [{ id: 'clock', error: 'invalid config' }];
    await loadMain(initial);
    mocks.openUrl.mockRejectedValueOnce(new Error('browser unavailable'));
    document.querySelector<HTMLButtonElement>('[data-github]')!.click();
    const fault = document.querySelector<HTMLElement>('[data-fault]')!;
    await vi.waitFor(() => expect(fault.textContent).toContain('browser unavailable'));

    const onState = mocks.listen.mock.calls.find(([name]) => name === 'aidget://main')![1];
    onState({ event: 'aidget://main', id: 0, payload: initial });
    expect(fault.textContent).toContain('browser unavailable');
    onState({ event: 'aidget://main', id: 0, payload: state('about') });
    expect(fault.hidden).toBe(false);
    expect(fault.textContent).toContain('browser unavailable');
  });

  it('restores the persisted page before the first reveal', async () => {
    await loadMain(state('about'));

    expect(pageButton('about').getAttribute('aria-current')).toBe('page');
    expect(document.querySelector<HTMLElement>('[data-view="about"]')!.hidden).toBe(false);
  });

  it('serializes rapid navigation writes so the last page wins', async () => {
    let finishFirstWrite: (() => void) | undefined;
    const written: string[] = [];
    mocks.invoke.mockImplementation(
      (
        command: string,
        args?: {
          preferences?: { lastPage: string };
        },
      ) => {
        if (command === 'main_state') {
          return Promise.resolve(state('integrations'));
        }
        if (command === 'main_painted') {
          return Promise.resolve(true);
        }
        if (command === 'integration_state') {
          return Promise.resolve({
            agents: [],
            mcpArgs: [],
            mcpCommand: 'aidget mcp',
            skillContent: '',
            skillDirectory: '',
          });
        }
        if (command === 'set_preferences') {
          written.push(args!.preferences!.lastPage);
          if (written.length === 1) {
            return new Promise<void>((resolve) => {
              finishFirstWrite = resolve;
            });
          }
          return Promise.resolve();
        }
        return Promise.resolve();
      },
    );

    await import('../../src/main');
    await vi.waitFor(() => {
      expect(pageButton('integrations').classList).toContain('is-active');
    });

    pageButton('local-widgets').click();
    await vi.waitFor(() => expect(written).toHaveLength(1));

    pageButton('about').click();
    await vi.waitFor(() => expect(pageButton('about').classList).toContain('is-active'));

    finishFirstWrite!();
    await vi.waitFor(() => expect(written).toHaveLength(2));
    expect(written.at(-1)).toBe('about');
  });

  it('opens the GitHub repository from the about page', async () => {
    await loadMain(state('about'));

    expect(document.querySelector('[data-message="discord"]')).toBeNull();
    const github = document.querySelector<HTMLButtonElement>('[data-github]')!;
    expect(github.disabled).toBe(false);

    github.click();
    await vi.waitFor(() => {
      expect(mocks.openUrl).toHaveBeenCalledWith('https://github.com/aidget-ai/aidget');
    });
  });

  it('updates an Agent chip after a successful integration change', async () => {
    let finishChange: ((state: IntegrationInfo) => void) | undefined;
    mocks.invoke.mockImplementation((command: string, args?: unknown) => {
      if (command === 'main_state') {
        return Promise.resolve(state('integrations'));
      }
      if (command === 'main_painted') {
        return Promise.resolve(true);
      }
      if (command === 'integration_state') {
        return Promise.resolve(integrationState(false));
      }
      if (command === 'set_agent_integration') {
        return new Promise((resolve) => {
          finishChange = (next) => resolve({ state: next, problems: [] });
        });
      }
      return Promise.resolve(args);
    });

    await import('../../src/main');
    const codex = await vi.waitFor(() => {
      const button = document.querySelector<HTMLButtonElement>('[data-agent="codex"]');
      expect(button).not.toBeNull();
      return button!;
    });
    codex.click();
    await vi.waitFor(() => {
      expect(document.querySelector('[data-view="integrations"]')!.getAttribute('aria-busy')).toBe(
        'true',
      );
    });
    expect(mocks.invoke).toHaveBeenCalledWith('set_agent_integration', {
      change: { agentIds: ['codex'], resource: 'mcp', enabled: true },
    });

    finishChange!(integrationState(true));
    await vi.waitFor(() => {
      expect(document.querySelector('[data-view="integrations"]')!.getAttribute('aria-busy')).toBe(
        'false',
      );
    });
    expect(
      document
        .querySelector<HTMLButtonElement>('[data-agent="codex"]')!
        .classList.contains('is-on'),
    ).toBe(true);
  });
});

it('opens local details from the card, preview or title without intercepting card actions', async () => {
  const initial = state('local-widgets');
  initial.language = 'zh-CN';
  initial.widgets = [
    {
      id: 'clock',
      title: 'Clock',
      packageName: null,
      revision: 1,
      token: 'token',
      width: 360,
      height: 220,
      enabled: false,
      createdAt: 1,
    },
  ];
  await loadMain(initial);
  const details = {
    config: {
      id: 'clock',
      title: 'Clock',
      logo: null,
      screenshots: [],
      platforms: ['macos'],
      size: { width: 360, height: 220 },
      dragMode: 'all',
      permissions: [],
      network: [],
    },
    description: null,
    preview: null,
  };
  mocks.invoke.mockImplementation((command: string) =>
    Promise.resolve(
      command === 'main_widget_details' ? details : command === 'main_state' ? initial : null,
    ),
  );
  const card = document.querySelector<HTMLElement>('[data-widget="clock"]')!;
  card.click();
  await vi.waitFor(() =>
    expect(mocks.invoke).toHaveBeenCalledWith('main_widget_details', { id: 'clock' }),
  );
  await vi.waitFor(() =>
    expect(document.querySelector<HTMLDialogElement>('.local-widget-dialog')?.open).toBe(true),
  );
  const dialog = document.querySelector<HTMLDialogElement>('.local-widget-dialog')!;
  expect(dialog.querySelector('.community-detail-identity')).toBeNull();
  expect(dialog.querySelector('.community-dialog-header button')).toBeNull();
  expect(dialog.querySelector('.community-permissions-empty')?.textContent).toBe('无特殊权限');
  dialog.querySelector<HTMLButtonElement>('.community-dialog-close')!.click();
  expect(dialog.open).toBe(false);
  const count = mocks.invoke.mock.calls.filter(
    ([command]) => command === 'main_widget_details',
  ).length;
  card.querySelector<HTMLButtonElement>('.switch')!.click();
  await Promise.resolve();
  expect(
    mocks.invoke.mock.calls.filter(([command]) => command === 'main_widget_details'),
  ).toHaveLength(count);
  card.querySelector<HTMLButtonElement>('.preview-stage')!.click();
  await vi.waitFor(() => expect(dialog.open).toBe(true));
  dialog.querySelector<HTMLButtonElement>('.community-dialog-close')!.click();
  card.querySelector<HTMLButtonElement>('.widget-title')!.click();
  await vi.waitFor(() => expect(dialog.open).toBe(true));
  dialog.querySelector<HTMLButtonElement>('.community-dialog-close')!.click();
});
