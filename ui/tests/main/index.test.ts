/// @vitest-environment happy-dom

import { readFileSync } from 'node:fs';

import type { EventCallback } from '@tauri-apps/api/event';
import { enable, isEnabled } from '@tauri-apps/plugin-autostart';
import { beforeEach, describe, expect, it, vi } from 'vite-plus/test';

import type { IntegrationInfo } from '../../src/main/integration';
import { translate } from '../../src/main/locale';
import type { MainState } from '../../src/main/model';
import type { DisplayInfo } from '../../src/types/rust';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(async (_event: string, _handler: EventCallback<MainState>) => () => {}),
  openUrl: vi.fn(async () => {}),
  startDragging: vi.fn(async () => {}),
  close: vi.fn(async () => {}),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: mocks.openUrl }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    startDragging: mocks.startDragging,
    close: mocks.close,
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
    RotateCcw: icon,
    ServerCog: icon,
    Settings: icon,
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
    language: 'en-US',
    trayClickAction: 'window',
    lastPage,
    githubToken: null,
    autoDownload: true,
    defaultDisplay: null,
    displays: [],
  };
}

function displayInfo(name: string, primary: boolean): DisplayInfo {
  return { name, primary, x: 0, y: 0, width: 1920, height: 1080, scale: 1 };
}

function integrationState(installed: boolean): IntegrationInfo {
  return {
    mcpCommand: '/Applications/aidget.app/Contents/MacOS/aidget',
    mcpDisplayCommand: '/Applications/aidget.app/Contents/MacOS/aidget',
    mcpArgs: ['mcp'],
    mcpDisplayArgs: ['mcp'],
    skillDirectory: '/Users/test/.aidget/skills/aidget',
    skillContent: '',
    dataDirectory: '/tmp/home',
    agents: [
      {
        id: 'codex',
        name: 'Codex',
        icon: 'codex-color',
        mcp: { path: '/Users/test/.codex/config.toml', installed, replaceable: false },
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

async function loadMain(initial: MainState, isDebugBuild = false) {
  mocks.invoke.mockImplementation((command: string, args?: { preferences?: unknown }) => {
    if (command === 'main_state') {
      return Promise.resolve(initial);
    }
    if (command === 'main_painted') {
      return Promise.resolve(true);
    }
    if (command === 'update_state') {
      return Promise.resolve({ state: 'idle', isDebugBuild, notes: null });
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
  mocks.startDragging.mockClear();
  mocks.close.mockReset().mockResolvedValue(undefined);
  Object.defineProperty(navigator, 'userAgent', { configurable: true, value: 'Linux' });
  vi.mocked(enable).mockClear();
  vi.mocked(isEnabled).mockReset().mockResolvedValue(false);
  prepareDocument();
});

describe('window chrome', () => {
  it.each(['Linux', 'Windows'])(
    'only shows a close button after the update action on %s',
    async (userAgent) => {
      Object.defineProperty(navigator, 'userAgent', { configurable: true, value: userAgent });
      await loadMain(state('integrations'));

      const controls = document.querySelector<HTMLElement>('[data-window-controls]')!;
      expect(controls.hidden).toBe(false);
      expect(controls.previousElementSibling?.hasAttribute('data-update-header')).toBe(true);
      expect(controls.closest('.page-header')).not.toBeNull();
      expect(
        [...controls.querySelectorAll('button')].map((button) => button.dataset.windowAction),
      ).toEqual(['close']);
    },
  );

  it('keeps native controls on macOS', async () => {
    Object.defineProperty(navigator, 'userAgent', { configurable: true, value: 'Macintosh' });
    await loadMain(state('integrations'));
    expect(document.querySelector<HTMLElement>('[data-window-controls]')!.hidden).toBe(true);
  });

  it.each(['Linux', 'Windows'])(
    'closes the %s window without starting a drag',
    async (userAgent) => {
      Object.defineProperty(navigator, 'userAgent', { configurable: true, value: userAgent });
      await loadMain(state('integrations'));
      const button = document.querySelector<HTMLButtonElement>('[data-window-action="close"]')!;
      button.dispatchEvent(new PointerEvent('pointerdown', { button: 0, bubbles: true }));
      button.click();
      await vi.waitFor(() => expect(mocks.close).toHaveBeenCalledOnce());
      expect(mocks.startDragging).not.toHaveBeenCalled();
      document
        .querySelector('.brand-bar')!
        .dispatchEvent(new PointerEvent('pointerdown', { button: 0, bubbles: true }));
      expect(mocks.startDragging).toHaveBeenCalledOnce();
    },
  );

  it('localizes the close button when the language changes', async () => {
    await loadMain(state('general'));
    const close = document.querySelector<HTMLButtonElement>('[data-window-action="close"]')!;
    expect(close.getAttribute('aria-label')).toBe('Close');

    const language = document.querySelector<HTMLSelectElement>('[data-language]')!;
    language.value = 'zh-CN';
    language.dispatchEvent(new Event('change'));
    await vi.waitFor(() => expect(close.getAttribute('aria-label')).toBe('关闭'));
    expect(close.title).toBe('关闭');
  });

  it('shows window operation failures', async () => {
    mocks.close.mockRejectedValueOnce(new Error('cannot close window'));
    await loadMain(state('integrations'));
    document.querySelector<HTMLButtonElement>('[data-window-action="close"]')!.click();
    await vi.waitFor(() => {
      expect(document.querySelector('[data-fault]')?.textContent).toContain('cannot close window');
    });
  });
});

describe('launch at login', () => {
  it('keeps development autostart disabled and explains why in the selected language', async () => {
    const initial = state('general');
    initial.language = 'zh-CN';
    vi.mocked(isEnabled).mockResolvedValue(true);
    await loadMain(initial, true);
    const toggle = document.querySelector<HTMLButtonElement>('[data-autostart]')!;
    const note = document.querySelector<HTMLElement>('[data-autostart-debug-note]')!;
    await vi.waitFor(() => expect(toggle.disabled).toBe(true));
    expect(toggle.getAttribute('aria-checked')).toBe('false');
    expect(note.hidden).toBe(false);
    expect(note.textContent).toBe('开发版不支持开机自启');
    toggle.click();
    expect(enable).not.toHaveBeenCalled();

    const language = document.querySelector<HTMLSelectElement>('[data-language]')!;
    language.value = 'en-US';
    language.dispatchEvent(new Event('change'));
    await vi.waitFor(() =>
      expect(note.textContent).toBe('Development builds cannot launch at login'),
    );
    expect(toggle.disabled).toBe(true);
  });

  it('retains the native autostart state and switch for release builds', async () => {
    vi.mocked(isEnabled).mockResolvedValue(true);
    await loadMain(state('general'));
    const toggle = document.querySelector<HTMLButtonElement>('[data-autostart]')!;
    await vi.waitFor(() => expect(toggle.getAttribute('aria-checked')).toBe('true'));
    expect(toggle.disabled).toBe(false);
    expect(document.querySelector<HTMLElement>('[data-autostart-debug-note]')!.hidden).toBe(true);
  });
});

describe('update row', () => {
  it('labels every button the About page can show', () => {
    prepareDocument();

    // The locale pass is the only thing that names a button here: a button
    // without a key renders with no text at all.
    for (const selector of ['[data-update-action]', '[data-update-header]']) {
      const button = document.querySelector<HTMLButtonElement>(selector);
      expect(button, selector).not.toBeNull();
      const label = button!.querySelector<HTMLElement>('[data-update-label]');
      const key = label?.dataset.message;
      expect(key, `${selector} needs a labelled data-message key`).toBeTruthy();
      expect(translate('en-US', key as never), `${selector} key ${key} has no text`).not.toBe('');
    }
  });
});

describe('language initialization', () => {
  it('offers only explicit Chinese and US English choices', async () => {
    await loadMain(state('general'));

    const select = document.querySelector<HTMLSelectElement>('[data-language]')!;
    expect([...select.options].map((option) => [option.value, option.text])).toEqual([
      ['en-US', 'English'],
      ['zh-CN', '简体中文'],
    ]);
  });

  it('does not follow later system-language changes', async () => {
    const initial = state('general');
    initial.language = 'zh-CN';
    Object.defineProperty(navigator, 'languages', {
      configurable: true,
      value: ['en-US'],
    });
    await loadMain(initial);

    window.dispatchEvent(new Event('languagechange'));

    expect(document.querySelector<HTMLSelectElement>('[data-language]')!.value).toBe('zh-CN');
    expect(document.documentElement.lang).toBe('zh-CN');
  });

  it('initializes once from system language order and merges pending state events', async () => {
    const initial = state('general');
    initial.language = null;
    const localized = { ...initial, language: 'zh-CN' as const };
    let finishInitialization: ((language: 'zh-CN') => void) | undefined;
    let mainStateCalls = 0;
    mocks.invoke.mockImplementation((command: string, args?: { language?: 'zh-CN' | 'en-US' }) => {
      if (command === 'main_state') {
        mainStateCalls += 1;
        return Promise.resolve(mainStateCalls === 1 ? initial : localized);
      }
      if (command === 'initialize_language') {
        expect(args?.language).toBe('zh-CN');
        return new Promise((resolve) => {
          finishInitialization = resolve;
        });
      }
      return Promise.resolve(true);
    });
    Object.defineProperty(navigator, 'languages', {
      configurable: true,
      value: ['ja-JP', 'zh-TW', 'en-US'],
    });

    await import('../../src/main');
    await vi.waitFor(() =>
      expect(mocks.invoke.mock.calls.some(([name]) => name === 'initialize_language')).toBe(true),
    );
    expect(document.querySelector<HTMLSelectElement>('[data-language]')!.disabled).toBe(true);

    const onState = mocks.listen.mock.calls.find(([name]) => name === 'aidget://main')![1];
    onState({ event: 'aidget://main', id: 1, payload: localized });
    const secondNull = { ...initial };
    onState({ event: 'aidget://main', id: 2, payload: secondNull });
    finishInitialization!('zh-CN');

    await vi.waitFor(() =>
      expect(document.querySelector<HTMLSelectElement>('[data-language]')!.disabled).toBe(false),
    );
    expect(document.querySelector<HTMLSelectElement>('[data-language]')!.value).toBe('zh-CN');
    expect(document.documentElement.lang).toBe('zh-CN');
    expect(mocks.invoke.mock.calls.filter(([name]) => name === 'initialize_language')).toHaveLength(
      1,
    );
    expect(mocks.invoke).toHaveBeenCalledWith('main_painted');
  });

  it('shows an initialization failure and keeps language controls disabled', async () => {
    const initial = state('general');
    initial.language = null;
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'main_state') {
        return Promise.resolve(initial);
      }
      if (command === 'initialize_language') {
        return Promise.reject(new Error('disk full'));
      }
      return Promise.resolve(true);
    });
    Object.defineProperty(navigator, 'languages', {
      configurable: true,
      value: ['en-US'],
    });

    await import('../../src/main');
    const fault = document.querySelector<HTMLElement>('[data-fault]')!;
    await vi.waitFor(() => expect(fault.textContent).toContain('disk full'));
    expect(fault.hidden).toBe(false);
    expect(document.querySelector<HTMLSelectElement>('[data-language]')!.disabled).toBe(true);
    expect(mocks.invoke).toHaveBeenCalledWith('main_painted');
  });

  it('rolls a failed manual language change back to the persisted locale', async () => {
    await loadMain(state('general'));
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'set_preferences') {
        return Promise.reject(new Error('write failed'));
      }
      return Promise.resolve({ state: 'idle' });
    });
    const select = document.querySelector<HTMLSelectElement>('[data-language]')!;
    select.value = 'zh-CN';
    select.dispatchEvent(new Event('change'));

    await vi.waitFor(() => expect(select.value).toBe('en-US'));
    expect(document.documentElement.lang).toBe('en-US');
    expect(document.querySelector<HTMLElement>('[data-fault]')!.textContent).toContain(
      'write failed',
    );
  });
});

describe('main window navigation', () => {
  it('sends the displayed revision when setting widget enabled state', async () => {
    const initial = state('local-widgets');
    initial.widgets = [
      {
        id: 'clock',
        title: 'Clock',
        packageName: null,
        revision: 7,
        token: 'token',
        width: 360,
        height: 220,
        enabled: true,
        createdAt: 1,
      },
    ];
    await loadMain(initial);
    const toggle = await vi.waitFor(() => {
      const value = document.querySelector<HTMLButtonElement>('[data-widget="clock"] .switch');
      expect(value).toBeTruthy();
      return value!;
    });
    toggle.click();
    await vi.waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('set_widget_enabled', {
        id: 'clock',
        enabled: false,
        baseRevision: 7,
      }),
    );
  });

  it('refreshes state after a rejected toggle without retrying the write', async () => {
    const initial = state('local-widgets');
    initial.widgets = [
      {
        id: 'clock',
        title: 'Clock',
        packageName: null,
        revision: 7,
        token: 'token',
        width: 360,
        height: 220,
        enabled: true,
        createdAt: 1,
      },
    ];
    await loadMain(initial);
    const toggle = await vi.waitFor(() => {
      const value = document.querySelector<HTMLButtonElement>('[data-widget="clock"] .switch');
      expect(value).toBeTruthy();
      return value!;
    });
    const refreshed = structuredClone(initial);
    refreshed.widgets[0].revision = 8;
    refreshed.widgets[0].enabled = false;
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'set_widget_enabled') {
        return Promise.reject('revision conflict');
      }
      if (command === 'main_state') {
        return Promise.resolve(refreshed);
      }
      return Promise.resolve(null);
    });
    toggle.click();
    await vi.waitFor(() => expect(toggle.getAttribute('aria-checked')).toBe('false'));
    const fault = document.querySelector<HTMLElement>('[data-fault]')!;
    expect(fault.hidden).toBe(false);
    expect(fault.textContent).toContain('revision conflict');
    expect(mocks.invoke.mock.calls.filter(([name]) => name === 'set_widget_enabled')).toHaveLength(
      1,
    );
    expect(toggle.disabled).toBe(false);
  });

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
      if (command === 'update_state') {
        return Promise.resolve({ state: 'idle', isDebugBuild: false, notes: null });
      }
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

  it('opens the GitHub token settings from the token label', async () => {
    await loadMain(state('general'));
    const button = document.querySelector<HTMLButtonElement>('[data-github-token-link]');
    expect(button?.getAttribute('aria-label')).toBe('Create a classic GitHub token');
    button!.click();
    expect(mocks.openUrl).toHaveBeenCalledWith('https://github.com/settings/tokens');
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
    const detail = `GET https://api.github.com/repos/${'long-path-'.repeat(50)}: HTTP 401 Unauthorized`;
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'main_state') {
        return Promise.resolve(initial);
      }
      if (command === 'main_painted') {
        return Promise.resolve(true);
      }
      if (command === 'set_github_token') {
        return Promise.reject(new Error(detail));
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
    expect(input.disabled).toBe(false);
    const error = document.querySelector<HTMLElement>('[data-github-token-error]')!;
    expect(error.textContent).toContain(detail);
    expect(error.getAttribute('role')).toBe('alert');
    input.value = 'corrected-token';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    expect(error.hidden).toBe(true);
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
            mcpDisplayArgs: [],
            mcpCommand: 'aidget mcp',
            mcpDisplayCommand: 'aidget mcp',
            skillContent: '',
            dataDirectory: '/tmp/home',
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
      minHostVersion: '0.1.1',
      logo: null,
      screenshots: [],
      platforms: ['macos'],
      size: { width: 360, height: 220 },
      dragMode: 'all',
      permissions: [],
      network: [],
    },
    compatibility: {
      state: 'compatible',
      currentVersion: '0.1.12',
      minHostVersion: '0.1.1',
      allowedByOverride: false,
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
  await vi.waitFor(() => expect(dialog.open).toBe(false));
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

it('navigates from an incompatible widget detail to the About update action', async () => {
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
      enabled: false,
      createdAt: 1,
    },
  ];
  await loadMain(initial);
  const details = {
    config: {
      id: 'clock',
      title: 'Clock',
      minHostVersion: '99.0.0',
      logo: null,
      screenshots: [],
      platforms: ['macos'],
      size: { width: 360, height: 220 },
      dragMode: 'all',
      permissions: [],
      network: [],
    },
    compatibility: {
      state: 'host_too_old',
      currentVersion: '0.1.12',
      minHostVersion: '99.0.0',
      allowedByOverride: false,
    },
    description: null,
    preview: null,
  };
  mocks.invoke.mockImplementation((command: string) =>
    Promise.resolve(command === 'main_widget_details' ? details : null),
  );
  document.querySelector<HTMLElement>('[data-widget="clock"]')!.click();
  const dialog = document.querySelector<HTMLDialogElement>('.local-widget-dialog')!;
  await vi.waitFor(() => expect(dialog.open).toBe(true));
  dialog.querySelector<HTMLButtonElement>('.community-host-incompatible button')!.click();
  await vi.waitFor(() => expect(pageButton('about').getAttribute('aria-current')).toBe('page'));
  expect(document.activeElement).toBe(document.querySelector<HTMLElement>('[data-update-action]'));
  expect(mocks.invoke.mock.calls.filter(([command]) => command === 'update_check')).toHaveLength(0);
});

describe('automatic download preference ownership', () => {
  it.each([true, false])(
    'restores failed persistence from %s after all completion microtasks and permits retry',
    async (original) => {
      const initial = state('about');
      initial.autoDownload = original;
      await loadMain(initial);
      let rejectSave: (reason: Error) => void = () => {};
      const save = new Promise<void>((_resolve, reject) => {
        rejectSave = reject;
      });
      const persisted = { ...initial };
      mocks.invoke.mockImplementation((command: string) => {
        if (command === 'set_preferences') {
          return save;
        }
        if (command === 'main_state') {
          return Promise.resolve(persisted);
        }
        return Promise.resolve({ state: 'idle' });
      });
      const toggle = document.querySelector<HTMLButtonElement>('[data-update-auto-download]')!;
      toggle.click();
      toggle.click();
      await Promise.resolve();
      expect(toggle.disabled).toBe(true);
      expect(mocks.invoke.mock.calls.filter(([c]) => c === 'set_preferences')).toHaveLength(1);
      rejectSave(new Error('disk full'));
      await vi.waitFor(() => expect(toggle.disabled).toBe(false));
      for (let i = 0; i < 20; i++) {
        await Promise.resolve();
      }
      expect(toggle.getAttribute('aria-checked')).toBe(String(original));
      expect(document.querySelector<HTMLElement>('[data-fault]')!.textContent).toContain(
        'disk full',
      );
      expect(persisted.autoDownload).toBe(original);
      mocks.invoke.mockImplementation(
        (command: string, args?: { preferences?: Pick<MainState, 'autoDownload'> }) => {
          if (command === 'set_preferences' && args?.preferences) {
            persisted.autoDownload = args.preferences.autoDownload;
          }
          return Promise.resolve({ state: 'idle' });
        },
      );
      toggle.click();
      await vi.waitFor(() => expect(toggle.disabled).toBe(false));
      expect(toggle.getAttribute('aria-checked')).toBe(String(!original));
      expect(persisted.autoDownload).toBe(!original);
      // A queued page write reads actual in-memory preferences, not the switch.
      pageButton('general').click();
      await vi.waitFor(() =>
        expect(mocks.invoke).toHaveBeenLastCalledWith(
          'set_preferences',
          expect.objectContaining({
            preferences: expect.objectContaining({ autoDownload: !original }),
          }),
        ),
      );
    },
  );
});

it('keeps the pending auto-download value when a stale main event arrives', async () => {
  const initial = state('about');
  initial.autoDownload = false;
  await loadMain(initial);
  let finishSave: () => void = () => {};
  const pending = new Promise<void>((resolve) => {
    finishSave = resolve;
  });
  mocks.invoke.mockImplementation((command: string) =>
    command === 'set_preferences' ? pending : Promise.resolve({ state: 'idle' }),
  );
  const toggle = document.querySelector<HTMLButtonElement>('[data-update-auto-download]')!;
  toggle.click();
  await Promise.resolve();
  expect(toggle.getAttribute('aria-checked')).toBe('true');

  // A main-state event still carrying the old value must not overwrite the
  // pending save. The page later writes currentPreferences(), so losing it here
  // would put the old value back on disk.
  const render = mocks.listen.mock.calls.find(([event]) => event === 'aidget://main')![1];
  render({ event: 'aidget://main', id: 1, payload: initial });
  expect(toggle.getAttribute('aria-checked')).toBe('true');

  finishSave();
  await vi.waitFor(() => expect(toggle.disabled).toBe(false));
  for (let i = 0; i < 20; i++) {
    await Promise.resolve();
  }
  expect(toggle.getAttribute('aria-checked')).toBe('true');

  pageButton('general').click();
  await vi.waitFor(() =>
    expect(mocks.invoke).toHaveBeenLastCalledWith(
      'set_preferences',
      expect.objectContaining({
        preferences: expect.objectContaining({ autoDownload: true }),
      }),
    ),
  );
});

it('saves the default display chosen in general settings', async () => {
  const initial = state('general');
  initial.displays = [displayInfo('Monitor #1', true), displayInfo('Monitor #2', false)];
  await loadMain(initial);
  const select = document.querySelector<HTMLSelectElement>('[data-default-display]')!;
  expect([...select.options].map((option) => option.value)).toEqual([
    '',
    'Monitor #1',
    'Monitor #2',
  ]);

  select.value = 'Monitor #2';
  select.dispatchEvent(new Event('change'));

  await vi.waitFor(() =>
    expect(mocks.invoke).toHaveBeenLastCalledWith('set_preferences', {
      preferences: expect.objectContaining({ defaultDisplay: 'Monitor #2' }),
    }),
  );
});

it('saves null when automatic placement is chosen again', async () => {
  const initial = state('general');
  initial.displays = [displayInfo('Monitor #1', true), displayInfo('Monitor #2', false)];
  initial.defaultDisplay = 'Monitor #2';
  await loadMain(initial);
  const select = document.querySelector<HTMLSelectElement>('[data-default-display]')!;
  expect(select.value).toBe('Monitor #2');

  select.value = '';
  select.dispatchEvent(new Event('change'));

  await vi.waitFor(() =>
    expect(mocks.invoke).toHaveBeenLastCalledWith('set_preferences', {
      preferences: expect.objectContaining({ defaultDisplay: null }),
    }),
  );
});

it('restores the saved default display when saving fails', async () => {
  const initial = state('general');
  initial.displays = [displayInfo('Monitor #1', true), displayInfo('Monitor #2', false)];
  await loadMain(initial);
  mocks.invoke.mockImplementation((command: string) =>
    command === 'set_preferences'
      ? Promise.reject(new Error('disk full'))
      : Promise.resolve({ state: 'idle' }),
  );
  const select = document.querySelector<HTMLSelectElement>('[data-default-display]')!;

  select.value = 'Monitor #2';
  select.dispatchEvent(new Event('change'));

  const fault = document.querySelector<HTMLElement>('[data-fault]')!;
  await vi.waitFor(() => expect(fault.textContent).toContain('disk full'));
  expect(select.value).toBe('');
});

it('refreshes the display options when a main event reports a new display', async () => {
  const initial = state('general');
  initial.displays = [displayInfo('Monitor #1', true)];
  await loadMain(initial);
  const select = document.querySelector<HTMLSelectElement>('[data-default-display]')!;
  expect(select.options).toHaveLength(2);

  const render = mocks.listen.mock.calls.find(([event]) => event === 'aidget://main')![1];
  render({
    event: 'aidget://main',
    id: 1,
    payload: { ...initial, displays: [...initial.displays, displayInfo('Monitor #2', false)] },
  });

  expect(select.options).toHaveLength(3);
});
