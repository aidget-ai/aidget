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
  const header = document.querySelector<HTMLButtonElement>('[data-update-header]')!;
  const page = createUpdatePage(
    about,
    header,
    () => 'en',
    async () => {},
    () => {},
  );
  page.setAboutActive(true);
  return page;
}

beforeEach(() => {
  mocks.invoke.mockReset().mockResolvedValue(snapshot('idle'));
  mocks.listen.mockClear();
  mocks.handlers.clear();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('update actions', () => {
  it('shows update progress in the version action and disables repeats', async () => {
    const page = mount();
    await page.refresh();
    const action = document.querySelector<HTMLButtonElement>('[data-update-action]')!;
    const label = action.querySelector<HTMLElement>('[data-update-label]')!;
    const icon = action.querySelector<HTMLElement>('[data-update-icon]')!;
    expect(label.textContent).toBe('Check for updates');
    expect(icon.hidden).toBe(true);

    // The launch check settles on its own, five seconds after launch: nothing the
    // window did can reveal that, so the backend announces it.
    mocks.handlers.get('aidget://update')!({
      payload: snapshot('downloading', { latest: '0.1.3', done: 1, total: 2 }),
    });

    expect(label.textContent).toBe('Downloading… 50%');
    expect(icon.hidden).toBe(false);
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
  const action = document.querySelector<HTMLButtonElement>('[data-update-action]')!;
  expect(action.querySelector('[data-update-label]')!.textContent).toBe('Update');
  action.click();
  await Promise.resolve();
  expect(mocks.invoke).toHaveBeenCalledWith('update_apply');
});
