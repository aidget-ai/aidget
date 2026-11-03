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
function mount(locale: 'en' | 'zh-CN' = 'en') {
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
    async () => {},
    () => {},
    () => {},
  );
  page.setAboutActive(true);
  return page;
}

beforeEach(() => {
  mocks.invoke.mockReset().mockResolvedValue(snapshot('idle'));
  mocks.listen.mockClear();
  mocks.handlers.clear();
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
    vi.useFakeTimers();
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
    vi.useFakeTimers();
    const page = mount();
    await page.refresh();
    expect(mocks.invoke).toHaveBeenCalledTimes(1);

    mocks.handlers.get('aidget://update')!({
      payload: snapshot('downloading', { done: 1, total: 2 }),
    });
    await vi.advanceTimersByTimeAsync(400);

    expect(mocks.invoke).toHaveBeenCalledTimes(2);
  });

  it('opens localized release notes without rendering them inline', async () => {
    const page = mount();
    await page.refresh();
    const notesRow = document.querySelector<HTMLElement>('[data-update-notes-row]')!;
    const open = document.querySelector<HTMLButtonElement>('[data-update-notes-open]')!;

    mocks.handlers.get('aidget://update')!({
      payload: snapshot('found', {
        latest: '0.1.3',
        notes: {
          from: '0.1.0',
          to: '0.1.3',
          releases: [
            {
              version: '0.1.3',
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
    expect(dialog.querySelector('h2')?.textContent).toBe('v0.1.3 release notes');
    expect(dialog.querySelector('[data-update-notes-range]')).toBeNull();
    expect(dialog.textContent).toContain('Fixes the updater.');
    expect(dialog.querySelector('.release-version')).toBeNull();
  });

  it('keeps the range and source versions when notes span multiple releases', async () => {
    const page = mount();
    await page.refresh();
    mocks.handlers.get('aidget://update')!({
      payload: snapshot('found', {
        latest: '0.1.3',
        notes: {
          from: '0.1.0',
          to: '0.1.3',
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
    expect(dialog.querySelector('[data-update-notes-range]')).toBeNull();
    expect(dialog.querySelector('.release-version')).toBeNull();
    expect(dialog.textContent).toContain('中文更新。');
    expect(dialog.textContent).not.toContain('English change.');

    dialog.querySelector<HTMLButtonElement>('.community-dialog-close')!.click();
    expect(dialog.open).toBe(false);
  });

  it('keeps the open release notes stable across progress updates', async () => {
    const page = mount();
    await page.refresh();
    const notes = {
      from: '0.1.0',
      to: '0.1.3',
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
