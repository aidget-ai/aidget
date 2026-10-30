/// @vitest-environment happy-dom

import { readFileSync } from 'node:fs';

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
function mount() {
  const start = mainHtml.indexOf('<body>') + '<body>'.length;
  const end = mainHtml.lastIndexOf('</body>');
  document.body.innerHTML = mainHtml.slice(start, end).replace(/<script[\s\S]*?<\/script>/g, '');
  const about = document.querySelector<HTMLElement>('[data-view="about"]')!;
  return createUpdatePage(
    about,
    () => 'en',
    async () => {},
    () => {},
  );
}

beforeEach(() => {
  mocks.invoke.mockReset().mockResolvedValue(snapshot('idle'));
  mocks.listen.mockClear();
  mocks.handlers.clear();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('update row', () => {
  it('shows the install button when the launch check announces itself', async () => {
    const page = mount();
    await page.refresh();
    const install = document.querySelector<HTMLElement>('[data-update-install]')!;
    expect(install.hidden).toBe(true);

    // The launch check settles on its own, five seconds after launch: nothing the
    // window did can reveal that, so the backend announces it.
    mocks.handlers.get('aidget://update')!({
      payload: snapshot('ready', { latest: '0.1.3', staged: true }),
    });

    expect(install.hidden).toBe(false);
    expect(document.querySelector('[data-update-status]')!.textContent).toContain('0.1.3');
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

  it('keeps the notes for the update and hides them when up to date', async () => {
    const page = mount();
    await page.refresh();
    const notesRow = document.querySelector<HTMLElement>('[data-update-notes-row]')!;

    mocks.handlers.get('aidget://update')!({
      payload: snapshot('found', { latest: '0.1.3', notes: 'Fixes the updater.' }),
    });
    expect(notesRow.hidden).toBe(false);
    expect(document.querySelector('[data-update-notes]')!.textContent).toBe('Fixes the updater.');

    mocks.handlers.get('aidget://update')!({ payload: snapshot('latest', { latest: '0.1.0' }) });
    expect(notesRow.hidden).toBe(true);
  });
});

it('renders the same staged contract verified by the backend policy caller', async () => {
  mount();
  // Rust runtime::update compares its real held-v2 snapshot with this fixture.
  mocks.handlers.get('aidget://update')!({ payload: { ...stagedContract, state: 'ready' } });
  expect(document.querySelector('[data-update-status]')!.textContent).toContain('2.0.0');
  expect(document.querySelector('[data-update-notes]')!.textContent).toBe('v2 notes');
  const install = document.querySelector<HTMLButtonElement>('[data-update-install]')!;
  expect(install.hidden).toBe(false);
  install.click();
  await Promise.resolve();
  expect(mocks.invoke).toHaveBeenCalledWith('update_install');
});
