// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vite-plus/test';

import { createCommunityMedia } from '../../src/main/community-media';
import { communityEntry } from '../helpers/community';
import { mockIntersections } from '../helpers/intersection';

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
const commit = 'a'.repeat(40);
beforeEach(() => {
  vi.restoreAllMocks();
  document.body.replaceChildren();
  mocks.invoke.mockReset().mockResolvedValue('data:image/png;base64,test');
  vi.spyOn(HTMLImageElement.prototype, 'src', 'set').mockImplementation(function (
    this: HTMLImageElement,
  ) {
    this.dispatchEvent(new Event('load'));
  });
});
function setup() {
  const intersections = mockIntersections(false);
  const root = document.createElement('section');
  document.body.append(root);
  const media = createCommunityMedia(() => 'en');
  const create = (index: number, detail = false) => {
    const view = media.create(communityEntry(index), 'screen.png', commit, root, detail);
    root.append(view.element);
    return view;
  };
  return { media, create, root, ...intersections };
}
describe('community media scheduling', () => {
  it('retries once per details opening after an unmounted image fails again', async () => {
    const { media, create, notify } = setup();
    mocks.invoke.mockRejectedValue(new Error('offline'));
    const first = create(1, true);
    notify(first.stage);
    await vi.waitFor(() => expect(first.stage.textContent).toBe('Screenshot unavailable'));
    media.retryFailed(communityEntry(1), commit);
    await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(first.stage.textContent).toBe('Screenshot unavailable'));
    media.render();
    first.update(communityEntry(1), 'screen.png', commit);
    await Promise.resolve();
    expect(mocks.invoke).toHaveBeenCalledTimes(2);
    first.dispose();
    media.retryFailed(communityEntry(1), commit);
    const reopened = create(1, true);
    notify(reopened.stage);
    await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledTimes(3));
    expect(mocks.invoke.mock.calls[2][1].refresh).toBe(true);
    await vi.waitFor(() => expect(reopened.stage.textContent).toBe('Screenshot unavailable'));
    expect(reopened.element.querySelector('button')).toBeNull();
  });
  it('retries a failed logo while retaining an already decoded screenshot', async () => {
    const { media, root, notify } = setup();
    const entry = communityEntry(1);
    entry.entry.config.logo = 'logo.png';
    entry.entry.files.push({ path: 'logo.png', size: 32, sha256: 'f'.repeat(64) });
    mocks.invoke.mockRejectedValueOnce(new Error('offline'));
    const logo = media.create(entry, 'logo.png', commit, root, false, true);
    const screenshot = media.create(entry, 'screen.png', commit, root);
    root.append(logo.element, screenshot.element);
    notify(logo.stage);
    notify(screenshot.stage);
    await vi.waitFor(() => expect(logo.stage.textContent).toBe('Logo unavailable'));
    await vi.waitFor(() => expect(screenshot.stage.querySelector('img')).toBeTruthy());
    const image = screenshot.stage.firstChild;
    media.retryFailed(entry, commit);
    media.retryFailed(entry, commit);
    await vi.waitFor(() => expect(logo.stage.querySelector('img')).toBeTruthy());
    expect(screenshot.stage.firstChild).toBe(image);
    expect(mocks.invoke).toHaveBeenCalledTimes(3);
    expect(mocks.invoke.mock.calls[2][1]).toEqual({
      commit,
      package: entry.entry.package.name,
      path: 'logo.png',
      refresh: true,
    });
  });
  it('uses the pinned dialog revision when a newer card shares its digest and the image is retried', async () => {
    const { media, create, notify } = setup();
    mocks.invoke.mockRejectedValueOnce(new Error('offline'));
    const card = create(1);
    const dialog = create(1, true);
    card.update(communityEntry(1), 'screen.png', 'b'.repeat(40));
    notify(card.stage);
    notify(dialog.stage);
    await vi.waitFor(() => expect(dialog.stage.textContent).toBe('Screenshot unavailable'));
    media.retryFailed(communityEntry(1), commit);
    await vi.waitFor(() => expect(dialog.stage.querySelector('img')).toBeTruthy());
    expect(mocks.invoke.mock.calls[1][1].commit).toBe(commit);
  });
  it('defers offscreen IPC and uses separate actual visibility and 300px preload boundaries', async () => {
    const { create, notify, observers } = setup();
    const view = create(1);
    await Promise.resolve();
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(observers.map((observer) => observer.rootMargin)).toEqual(['0px', '300px']);
    notify(view.stage, false, true);
    await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledTimes(1));
    await vi.waitFor(() => expect(view.stage.querySelector('img')).toBeTruthy());
  });
  it('limits concurrency to four and starts visible detail images ahead of list and nearby media', async () => {
    const { create, notify } = setup();
    const pending: ((value: string) => void)[] = [];
    mocks.invoke.mockImplementation(() => new Promise<string>((resolve) => pending.push(resolve)));
    const nearby = create(1);
    const lists = [2, 3, 4, 5].map((index) => create(index));
    const detail = create(6, true);
    notify(nearby.stage, false, true);
    for (const view of lists) {
      notify(view.stage);
    }
    notify(detail.stage);
    await vi.waitFor(() => expect(pending).toHaveLength(4));
    const packages = () => mocks.invoke.mock.calls.map(([, args]) => args.package);
    expect(packages()).toEqual([6, 2, 3, 4].map((index) => `@aidget-community/widget-${index}`));
    pending[0]('data:image/png;base64,test');
    await vi.waitFor(() => expect(pending).toHaveLength(5));
    expect(packages()[4]).toBe('@aidget-community/widget-5');
    pending[1]('data:image/png;base64,test');
    await vi.waitFor(() => expect(pending).toHaveLength(6));
    expect(packages()[5]).toBe('@aidget-community/widget-1');
    for (const resolve of pending) {
      resolve('data:image/png;base64,test');
    }
  });
  it('shares a digest across list and dialog and preserves the decoded image on revision changes', async () => {
    const { create, notify } = setup();
    const first = create(1);
    const second = create(1, true);
    notify(first.stage);
    notify(second.stage);
    await vi.waitFor(() => expect(first.stage.querySelector('img')).toBeTruthy());
    const image = first.stage.firstChild;
    first.update(communityEntry(1), 'screen.png', 'b'.repeat(40));
    first.render();
    expect(first.stage.firstChild).toBe(image);
    expect(second.stage.querySelector('img')).toBeTruthy();
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    first.update(communityEntry(2), 'screen.png', 'b'.repeat(40));
    await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledTimes(2));
  });
  it('cancels orphaned queued work and suspends hidden panels without cancelling active downloads', async () => {
    const { media, create, notify } = setup();
    const pending: ((value: string) => void)[] = [];
    mocks.invoke.mockImplementation(() => new Promise<string>((resolve) => pending.push(resolve)));
    const views = Array.from({ length: 6 }, (_, index) => create(index));
    for (const view of views) {
      notify(view.stage);
    }
    await vi.waitFor(() => expect(pending).toHaveLength(4));
    views[4].dispose();
    media.setActive(false);
    for (const resolve of pending) {
      resolve('data:image/png;base64,test');
    }
    await vi.waitFor(() => expect(views[0].stage.querySelector('img')).toBeTruthy());
    expect(mocks.invoke).toHaveBeenCalledTimes(4);
    media.setActive(true);
    await vi.waitFor(() => expect(pending).toHaveLength(5));
    expect(mocks.invoke.mock.calls[4][1].package).toBe('@aidget-community/widget-5');
    pending[4]('data:image/png;base64,test');
  });
  it('retries only the failed digest and updates all its subscribers without an automatic retry loop', async () => {
    const { media, create, notify } = setup();
    mocks.invoke.mockRejectedValueOnce(new Error('offline'));
    const first = create(1);
    const shared = create(1, true);
    const other = create(2);
    for (const view of [first, shared, other]) {
      notify(view.stage);
    }
    await vi.waitFor(() => expect(first.stage.textContent).toBe('Screenshot unavailable'));
    expect(shared.stage.textContent).toBe('Screenshot unavailable');
    expect(mocks.invoke).toHaveBeenCalledTimes(2);
    media.retryFailed(communityEntry(1), commit);
    await vi.waitFor(() => expect(first.stage.querySelector('img')).toBeTruthy());
    expect(shared.stage.querySelector('img')).toBeTruthy();
    expect(other.stage.querySelector('img')).toBeTruthy();
    expect(mocks.invoke).toHaveBeenCalledTimes(3);
    expect(mocks.invoke.mock.calls[2][1]).toEqual({
      commit,
      package: '@aidget-community/widget-1',
      path: 'screen.png',
      refresh: true,
    });
    expect(first.element.querySelector('button')).toBeNull();
  });
  it('makes decoding failures retryable and follows a replacement dialog scroll root', async () => {
    const { media, create, notify, root, observers } = setup();
    const view = create(1, true);
    notify(view.stage);
    await vi.waitFor(() => expect(view.stage.querySelector('img')).toBeTruthy());
    view.stage.querySelector('img')!.dispatchEvent(new Event('error'));
    expect(view.stage.textContent).toBe('Screenshot unavailable');
    const content = document.createElement('div');
    root.append(content);
    content.append(view.element);
    view.setRoot(content);
    expect(observers.slice(-2).every((observer) => observer.root === content)).toBe(true);
    notify(view.stage);
    media.retryFailed(communityEntry(1), commit);
    await vi.waitFor(() => expect(view.stage.querySelector('img')).toBeTruthy());
    media.render();
    expect(view.element.querySelector('button')).toBeNull();
  });
});
