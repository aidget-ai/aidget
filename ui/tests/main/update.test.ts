/// @vitest-environment happy-dom

import { readFileSync } from 'node:fs';

import { ArrowUp, createIcons } from 'lucide';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vite-plus/test';

import { createUpdatePage, type UpdateSnapshot } from '../../src/main/update';
import stagedContract from '../fixtures/update-staged.json';

type Handler = (event: { payload: UpdateSnapshot }) => void;

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(async (_event: string, _handler: Handler) => () => {}),
  handlers: new Map<string, Handler>(),
  autoDownload: vi.fn(async (_enabled: boolean) => {}),
  fault: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: Handler) => {
    mocks.handlers.set(event, handler);
    return () => {};
  }),
}));

const mainHtml = readFileSync('ui/main.html', 'utf8');

function snapshot(state: UpdateSnapshot['state'], extra: Partial<UpdateSnapshot> = {}) {
  return {
    state,
    isDebugBuild: false,
    current: 'v0.1.0',
    latest: null,
    error: null,
    done: null,
    total: null,
    notes: null,
    staged: false,
    ...extra,
  } satisfies UpdateSnapshot;
}

/// Mount the real page markup and create the row against it.
function mount(locale: 'en-US' | 'zh-CN' = 'en-US') {
  const start = mainHtml.indexOf('<body>') + '<body>'.length;
  const end = mainHtml.lastIndexOf('</body>');
  document.body.innerHTML = mainHtml.slice(start, end).replace(/<script[\s\S]*?<\/script>/g, '');
  for (const button of document.querySelectorAll<HTMLElement>(
    '[data-update-action], [data-update-header]',
  )) {
    createIcons({ icons: { ArrowUp }, root: button });
  }
  const about = document.querySelector<HTMLElement>('[data-view="about"]')!;
  const header = document.querySelector<HTMLButtonElement>('[data-update-header]')!;
  const page = createUpdatePage(
    about,
    header,
    () => locale,
    mocks.autoDownload,
    mocks.fault,
    () => {},
  );
  page.setAboutActive(true);
  return page;
}

beforeEach(() => {
  vi.useFakeTimers();
  mocks.invoke.mockReset().mockResolvedValue(snapshot('idle'));
  mocks.listen.mockClear();
  mocks.handlers.clear();
  mocks.autoDownload.mockClear();
  mocks.fault.mockClear();
  vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (
    this: HTMLDialogElement,
  ) {
    this.open = true;
  });
  vi.spyOn(HTMLDialogElement.prototype, 'close').mockImplementation(function (
    this: HTMLDialogElement,
  ) {
    this.open = false;
  });
});

afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('update actions', () => {
  it('does not label a version recheck as downloading', async () => {
    const page = mount();
    await page.refresh();
    const action = document.querySelector<HTMLButtonElement>('[data-update-action]')!;
    const label = action.querySelector<HTMLElement>('[data-update-label]')!;

    mocks.handlers.get('aidget://update')!({
      payload: snapshot('checking', { latest: '0.1.3' }),
    });

    expect(label.textContent).toBe('Checking…');
    expect(action.dataset.updateMode).toBe('progress');
  });

  it('keeps the release-notes entry available from the About page', async () => {
    const page = mount();
    await page.refresh();

    const row = document.querySelector<HTMLElement>('[data-update-notes-row]')!;
    const open = document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!;
    expect(row.hidden).toBe(false);
    expect(open.textContent).toBe('Open');
    expect(open.disabled).toBe(true);
  });

  it('hides automatic update actions in a debug build while keeping release notes available', async () => {
    mocks.invoke.mockResolvedValueOnce(snapshot('idle', { isDebugBuild: true }));
    const page = mount('zh-CN');
    await page.refresh();

    const action = document.querySelector<HTMLButtonElement>('[data-update-action]')!;
    const header = document.querySelector<HTMLButtonElement>('[data-update-header]')!;
    const toggle = document.querySelector<HTMLButtonElement>('[data-update-auto-download]')!;
    const versionNote = document.querySelector<HTMLElement>('[data-update-version-debug-note]')!;
    const debugNote = document.querySelector<HTMLElement>('[data-update-debug-note]')!;
    const notesNote = document.querySelector<HTMLElement>('[data-update-notes-limit-note]')!;
    const open = document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!;

    expect(action.hidden).toBe(true);
    page.setAboutActive(false);
    expect(header.hidden).toBe(true);
    expect(toggle.disabled).toBe(true);
    expect(toggle.getAttribute('aria-busy')).toBe('false');
    expect(toggle.getAttribute('aria-checked')).toBe('false');
    expect(toggle.getAttribute('aria-describedby')).toBe(debugNote.id);
    expect(versionNote.hidden).toBe(false);
    expect(versionNote.textContent).toBe(
      '开发版固定为 0.1.1 版本，发布正式版时会动态注入新版本号。',
    );
    expect(debugNote.hidden).toBe(false);
    expect(debugNote.textContent).toBe('开发版不支持自动下载更新');
    expect(notesNote.hidden).toBe(false);
    expect(notesNote.textContent).toBe('开发版只展示近 5 个稳定版本更新内容');
    expect(open.disabled).toBe(false);
    expect(open.textContent).toBe('打开');

    page.setAutoDownload(true);
    expect(toggle.getAttribute('aria-checked')).toBe('false');
    toggle.click();
    expect(mocks.autoDownload).not.toHaveBeenCalled();
  });

  it('loads debug release notes on demand once and opens the existing dialog', async () => {
    const notes = {
      from: '7.0.0',
      to: '8.0.0',
      maintenanceVersions: [],
      maintenanceOnly: false,
      releases: [{ version: '8.0.0', body: '## Changes\n\n- Faster launch.' }],
    };
    let finishNotes: (value: UpdateSnapshot) => void = () => {};
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'update_state') {
        return Promise.resolve(snapshot('idle', { isDebugBuild: true }));
      }
      if (command === 'update_check') {
        return new Promise<UpdateSnapshot>((resolve) => {
          finishNotes = resolve;
        });
      }
      return Promise.resolve(snapshot('idle', { isDebugBuild: true }));
    });
    const page = mount();
    await page.refresh();
    const open = document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!;

    open.click();
    open.click();

    expect(mocks.invoke.mock.calls.filter(([command]) => command === 'update_check')).toHaveLength(
      1,
    );
    expect(open.disabled).toBe(true);
    expect(open.getAttribute('aria-busy')).toBe('true');
    expect(open.textContent).toBe('Loading…');

    finishNotes(snapshot('source', { isDebugBuild: true, notes }));
    await vi.waitFor(() =>
      expect(document.querySelector<HTMLDialogElement>('[data-update-notes-dialog]')?.open).toBe(
        true,
      ),
    );
    expect(document.querySelector('[data-update-notes-dialog]')!.textContent).toContain(
      'Faster launch.',
    );
  });

  it('refreshes cached debug release notes before the first open', async () => {
    const cachedNotes = {
      from: '3.0.0',
      to: '5.0.0',
      maintenanceVersions: [],
      maintenanceOnly: false,
      releases: [{ version: '5.0.0', body: '## Changes\n\n- Cached content.' }],
    };
    const currentNotes = {
      from: '7.0.0',
      to: '8.0.0',
      maintenanceVersions: [],
      maintenanceOnly: false,
      releases: [{ version: '8.0.0', body: '## Changes\n\n- Current content.' }],
    };
    mocks.invoke
      .mockResolvedValueOnce(snapshot('idle', { isDebugBuild: true, notes: cachedNotes }))
      .mockResolvedValueOnce(snapshot('source', { isDebugBuild: true, notes: currentNotes }));
    const page = mount();
    await page.refresh();

    document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!.click();

    await vi.waitFor(() =>
      expect(document.querySelector('[data-update-notes-dialog]')?.textContent).toContain(
        'Current content.',
      ),
    );
    expect(mocks.invoke.mock.calls.filter(([command]) => command === 'update_check')).toHaveLength(
      1,
    );
  });

  it('renders the maintenance notice after a debug on-demand check', async () => {
    mocks.invoke
      .mockResolvedValueOnce(snapshot('idle', { isDebugBuild: true }))
      .mockResolvedValueOnce(
        snapshot('source', {
          isDebugBuild: true,
          notes: {
            from: '1.0.0',
            to: '2.0.0',
            maintenanceVersions: ['2.0.0'],
            maintenanceOnly: true,
            releases: [],
          },
        }),
      );
    const page = mount('zh-CN');
    await page.refresh();

    document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!.click();

    await vi.waitFor(() =>
      expect(document.querySelector('.release-notes-maintenance')?.textContent).toBe(
        '本次更新仅内部维护，应用功能和使用方式保持不变。',
      ),
    );
    expect(document.querySelector('.release-version')).toBeNull();
  });

  it('reports a failed debug release-notes request without opening cached content', async () => {
    const cachedNotes = {
      from: '7.0.0',
      to: '8.0.0',
      maintenanceVersions: [],
      maintenanceOnly: false,
      releases: [{ version: '8.0.0', body: '## Changes\n\n- Old content.' }],
    };
    mocks.invoke
      .mockResolvedValueOnce(snapshot('idle', { isDebugBuild: true }))
      .mockResolvedValueOnce(
        snapshot('error', {
          isDebugBuild: true,
          error: 'update feed unavailable',
          notes: cachedNotes,
        }),
      );
    const page = mount();
    await page.refresh();

    document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!.click();

    await vi.waitFor(() => expect(mocks.fault).toHaveBeenCalledWith('update feed unavailable'));
    expect(document.querySelector<HTMLDialogElement>('[data-update-notes-dialog]')!.open).toBe(
      false,
    );
    expect(document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!.disabled).toBe(
      false,
    );
  });

  it('shows an empty debug release-notes state when no stable release is published', async () => {
    mocks.invoke
      .mockResolvedValueOnce(snapshot('idle', { isDebugBuild: true }))
      .mockResolvedValueOnce(snapshot('no_release', { isDebugBuild: true }));
    const page = mount('zh-CN');
    await page.refresh();

    document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!.click();

    await vi.waitFor(() => {
      const empty = document.querySelector<HTMLElement>('[data-update-notes-empty]')!;
      expect(empty.hidden).toBe(false);
      expect(empty.textContent).toBe('暂无更新内容');
    });
    expect(document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!.disabled).toBe(
      true,
    );
    expect(document.querySelector<HTMLDialogElement>('[data-update-notes-dialog]')!.open).toBe(
      false,
    );
  });

  it('shows update progress in the version action and disables repeats', async () => {
    const page = mount();
    await page.refresh();
    const action = document.querySelector<HTMLButtonElement>('[data-update-action]')!;
    const label = action.querySelector<HTMLElement>('[data-update-label]')!;
    expect(label.textContent).toBe('Check for updates');
    expect(action.querySelector('.update-action-icon')).toBeNull();

    // The launch check settles on its own, five seconds after launch: nothing the
    // window did can reveal that, so the backend announces it.
    mocks.handlers.get('aidget://update')!({
      payload: snapshot('downloading', { latest: '0.1.3', done: 1, total: 2 }),
    });

    expect(label.textContent).toBe('Downloading… 50%');
    expect(action.querySelector('.update-action-icon')).toBeNull();
    expect(action.dataset.updateMode).toBe('progress');
    expect(action.disabled).toBe(true);
    expect(document.querySelector('[data-update-status]')!.textContent).toContain('0.1.3');
  });

  it('shows the same update action in the header outside About', async () => {
    const page = mount();
    await page.refresh();
    const header = document.querySelector<HTMLButtonElement>('[data-update-header]')!;

    mocks.handlers.get('aidget://update')!({
      payload: snapshot('ready', { latest: '0.1.3', staged: true }),
    });
    expect(header.hidden).toBe(true);

    page.setAboutActive(false);
    expect(header.hidden).toBe(false);
    expect(header.querySelector('[data-update-label]')!.textContent).toBe('Update');
    expect(header.dataset.updateMode).toBe('update');
    expect(header.querySelector('.update-action-icon')!.hasAttribute('hidden')).toBe(false);
  });

  it('applies the found update instead of downloading and installing separately', async () => {
    const page = mount();
    await page.refresh();
    const action = document.querySelector<HTMLButtonElement>('[data-update-action]')!;
    const label = action.querySelector<HTMLElement>('[data-update-label]')!;
    mocks.invoke.mockResolvedValueOnce(snapshot('found', { latest: '0.1.3' }));

    mocks.handlers.get('aidget://update')!({
      payload: snapshot('found', { latest: '0.1.3' }),
    });
    expect(label.textContent).toBe('Update');
    expect(action.dataset.updateMode).toBe('update');
    action.click();
    await Promise.resolve();

    expect(mocks.invoke).toHaveBeenLastCalledWith('update_apply');
  });

  it('shows the latest result for three seconds before returning to check', async () => {
    const page = mount();
    await page.refresh();
    const action = document.querySelector<HTMLButtonElement>('[data-update-action]')!;
    const label = action.querySelector<HTMLElement>('[data-update-label]')!;

    mocks.handlers.get('aidget://update')!({
      payload: snapshot('latest', { latest: '0.1.0' }),
    });
    expect(label.textContent).toBe('Up to date');
    expect(action.dataset.updateMode).toBe('check');

    await vi.advanceTimersByTimeAsync(2999);
    expect(label.textContent).toBe('Up to date');
    await vi.advanceTimersByTimeAsync(1);
    expect(label.textContent).toBe('Check for updates');
  });

  it('reads the state again while a download runs', async () => {
    const page = mount();
    await page.refresh();
    expect(mocks.invoke).toHaveBeenCalledTimes(1);

    mocks.handlers.get('aidget://update')!({
      payload: snapshot('downloading', { done: 1, total: 2 }),
    });
    await vi.advanceTimersByTimeAsync(400);

    expect(mocks.invoke).toHaveBeenCalledTimes(2);
  });

  it('shows the upgrade range for a single release without rendering notes inline', async () => {
    const page = mount();
    await page.refresh();
    const notesRow = document.querySelector<HTMLElement>('[data-update-notes-row]')!;
    const open = document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!;

    mocks.handlers.get('aidget://update')!({
      payload: snapshot('found', {
        current: '0.1.3',
        latest: '0.1.4',
        notes: {
          from: '0.1.3',
          to: '0.1.4',
          maintenanceVersions: [],
          maintenanceOnly: false,
          releases: [
            {
              version: '0.1.4',
              body: '## Improvements\n\n- Fixes the updater.',
            },
          ],
        },
      }),
    });
    expect(notesRow.hidden).toBe(false);
    expect(open.disabled).toBe(false);
    expect(document.querySelector('[data-update-notes]')).toBeNull();

    open.click();
    const dialog = document.querySelector<HTMLDialogElement>('[data-update-notes-dialog]')!;
    expect(dialog.open).toBe(true);
    expect(dialog.querySelector('h2')?.textContent).toBe('v0.1.4 release notes');
    expect(dialog.querySelector('[data-update-notes-range]')?.textContent).toBe('v0.1.3 → v0.1.4');
    expect(dialog.textContent).toContain('Fixes the updater.');
    expect(dialog.querySelector('.release-version')).toBeNull();
  });

  it('renders one maintenance notice for an all-internal upgrade range', async () => {
    const page = mount('zh-CN');
    await page.refresh();
    mocks.handlers.get('aidget://update')!({
      payload: snapshot('found', {
        latest: '0.1.6',
        notes: {
          from: '0.1.1',
          to: '0.1.6',
          maintenanceVersions: ['0.1.6', '0.1.5'],
          maintenanceOnly: true,
          releases: [],
        },
      }),
    });

    const open = document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!;
    expect(open.disabled).toBe(false);
    open.click();
    const dialog = document.querySelector<HTMLDialogElement>('[data-update-notes-dialog]')!;
    expect(dialog.open).toBe(true);
    expect(dialog.querySelector('[data-update-notes-range]')?.textContent).toBe('v0.1.1 → v0.1.6');
    expect(dialog.querySelector('.release-notes-maintenance')?.textContent).toBe(
      '本次更新仅内部维护，应用功能和使用方式保持不变。',
    );
    expect(dialog.querySelector('.release-version')).toBeNull();
    expect(dialog.querySelectorAll('.release-notes-category')).toHaveLength(0);
  });

  it.each(['0.1.4', 'v0.1.4'])(
    'omits the range when %s is also the target version',
    async (from) => {
      const page = mount();
      await page.refresh();
      mocks.handlers.get('aidget://update')!({
        payload: snapshot('latest', {
          current: '0.1.4',
          latest: '0.1.4',
          notes: {
            from,
            to: '0.1.4',
            maintenanceVersions: [],
            maintenanceOnly: false,
            releases: [{ version: '0.1.4', body: '## Improvements\n\n- Latest change.' }],
          },
        }),
      });

      document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!.click();
      const dialog = document.querySelector<HTMLDialogElement>('[data-update-notes-dialog]')!;
      expect(dialog.querySelector('h2')?.textContent).toBe('v0.1.4 release notes');
      expect(dialog.querySelector('[data-update-notes-range]')).toBeNull();
      expect(dialog.querySelector('.release-version')).toBeNull();
    },
  );

  it('keeps the range and source versions when notes span multiple releases', async () => {
    const page = mount();
    await page.refresh();
    mocks.handlers.get('aidget://update')!({
      payload: snapshot('found', {
        latest: '0.1.3',
        notes: {
          from: '0.1.0',
          to: '0.1.3',
          maintenanceVersions: [],
          maintenanceOnly: false,
          releases: [
            {
              version: '0.1.3',
              body: '## Improvements\n\n- Newest change.',
            },
            {
              version: '0.1.2',
              body: '## Improvements\n\n- Older change.',
            },
          ],
        },
      }),
    });

    document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!.click();
    const dialog = document.querySelector<HTMLDialogElement>('[data-update-notes-dialog]')!;
    expect(dialog.querySelector('[data-update-notes-range]')?.textContent).toBe('v0.1.0 → v0.1.3');
    expect(
      [...dialog.querySelectorAll('.release-version')].map((badge) => badge.textContent),
    ).toEqual(['v0.1.3', 'v0.1.2']);
  });

  it('uses the configured language and closes the release-notes dialog', async () => {
    const page = mount('zh-CN');
    await page.refresh();
    mocks.handlers.get('aidget://update')!({
      payload: snapshot('found', {
        latest: '0.2.0',
        notes: {
          from: '0.1.0',
          to: '0.2.0',
          maintenanceVersions: [],
          maintenanceOnly: false,
          releases: [
            {
              version: '0.2.0',
              body: `# Changelog

## Features

- English change.

<!-- lang:zh -->

# Changelog

## 新功能

- 中文更新。

### 安装说明`,
            },
          ],
        },
      }),
    });

    document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!.click();
    const dialog = document.querySelector<HTMLDialogElement>('[data-update-notes-dialog]')!;
    expect(dialog.querySelector('h2')?.textContent).toBe('v0.2.0 更新内容');
    expect(dialog.querySelector('[data-update-notes-range]')?.textContent).toBe('v0.1.0 → v0.2.0');
    expect(dialog.querySelector('.release-version')).toBeNull();
    expect(dialog.textContent).toContain('中文更新。');
    expect(dialog.textContent).not.toContain('English change.');

    dialog.querySelector<HTMLButtonElement>('.community-dialog-close')!.click();
    await vi.advanceTimersByTimeAsync(240);
    expect(dialog.open).toBe(false);
  });

  it('keeps the open release notes stable across progress updates', async () => {
    const page = mount();
    await page.refresh();
    const notes = {
      from: '0.1.0',
      to: '0.1.3',
      maintenanceVersions: [],
      maintenanceOnly: false,
      releases: [
        {
          version: '0.1.3',
          body: '## Improvements\n\n- Fixes the updater.',
        },
      ],
    };
    mocks.handlers.get('aidget://update')!({
      payload: snapshot('downloading', { latest: '0.1.3', done: 1, total: 10, notes }),
    });

    document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!.click();
    const dialog = document.querySelector<HTMLDialogElement>('[data-update-notes-dialog]')!;
    const content = dialog.querySelector<HTMLElement>('.release-notes-content')!;
    const close = dialog.querySelector<HTMLButtonElement>('.community-dialog-close')!;
    content.scrollTop = 200;
    close.focus();

    mocks.handlers.get('aidget://update')!({
      payload: snapshot('downloading', { latest: '0.1.3', done: 2, total: 10, notes }),
    });

    expect(dialog.querySelector('.release-notes-content')).toBe(content);
    expect(content.scrollTop).toBe(200);
    expect(document.activeElement).toBe(close);

    mocks.handlers.get('aidget://update')!({
      payload: snapshot('downloading', {
        latest: '0.1.3',
        done: 3,
        total: 10,
        notes: {
          ...notes,
          releases: [{ ...notes.releases[0], body: '## Improvements\n\n- Corrected note.' }],
        },
      }),
    });
    expect(dialog.textContent).toContain('Corrected note.');
  });
});

it('renders the same staged contract verified by the backend policy caller', async () => {
  mount();
  // Rust runtime::update compares its real held-v2 snapshot with this fixture.
  mocks.handlers.get('aidget://update')!({ payload: { ...stagedContract, state: 'ready' } });
  expect(document.querySelector('[data-update-status]')!.textContent).toContain('2.0.0');
  document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!.click();
  expect(
    document.querySelector<HTMLDialogElement>('[data-update-notes-dialog]')!.textContent,
  ).toContain('v2 notes');
  const action = document.querySelector<HTMLButtonElement>('[data-update-action]')!;
  expect(action.querySelector('[data-update-label]')!.textContent).toBe('Update');
  action.click();
  await Promise.resolve();
  expect(mocks.invoke).toHaveBeenCalledWith('update_apply');
});
