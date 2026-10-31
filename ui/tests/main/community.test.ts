import type { Channel } from '@tauri-apps/api/core';
import { mockIPC } from '@tauri-apps/api/mocks';
// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vite-plus/test';

import type { CommunitySnapshot, DownloadCount } from '../../src/types/rust';
import { mockIntersections } from '../helpers/intersection';
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), openUrl: vi.fn() }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: mocks.openUrl }));
vi.mock('@tauri-apps/api/core', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@tauri-apps/api/core')>()),
  invoke: mocks.invoke,
}));
import { createCommunityPage, weeklyDownloads } from '../../src/main/community';
const snapshot: CommunitySnapshot = {
  commit: 'a'.repeat(40),
  widgets: [
    {
      entry: {
        publishedAt: '2026-10-25T06:49:57Z',
        package: {
          name: '@aidget-community/clock',
          version: '1.0.0',
          description: 'A safe desktop clock',
          author: 'Community',
          repository: null,
          license: 'MIT',
          files: [],
        },
        config: {
          id: 'clock',
          title: 'Clock',
          logo: null,
          screenshots: ['assets/screen.png'],
          platforms: ['macos', 'linux', 'windows'],
          size: { width: 360, height: 220 },
          dragMode: 'all',
          permissions: ['script'],
          network: ['https://api.example.com'],
        },
        files: [{ path: 'assets/screen.png', size: 12, sha256: 'c'.repeat(64) }],
      },
      installed: false,
      idConflict: false,
      supported: true,
    },
  ],
};
beforeEach(() => {
  vi.restoreAllMocks();
  mockIntersections();
  mockIPC(() => {});
  document.body.replaceChildren();
  localStorage.removeItem('aidget.communityBannerDismissed');
  mocks.invoke.mockReset();
  mocks.openUrl.mockReset().mockResolvedValue(undefined);
  vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (
    this: HTMLDialogElement,
  ) {
    this.open = true;
  });
  vi.spyOn(HTMLDialogElement.prototype, 'close').mockImplementation(function (
    this: HTMLDialogElement,
  ) {
    this.open = false;
    this.dispatchEvent(new Event('close'));
  });
  mocks.invoke.mockImplementation((command: string) => {
    if (command === 'community_cached') {
      return Promise.resolve(null);
    }
    if (command === 'community_list') {
      return Promise.resolve(structuredClone(snapshot));
    }
    if (command === 'community_downloads') {
      return Promise.resolve([{ package: '@aidget-community/clock', weekly: 0, error: null }]);
    }
    if (command === 'community_media') {
      return Promise.resolve('data:image/png;base64,iVBORw0KGgo=');
    }
    return Promise.resolve();
  });
});
function page(hasGithubToken = false) {
  const root = document.createElement('section');
  document.body.append(root);
  const installed = vi.fn(async () => {});
  const notify = vi.fn();
  const page = createCommunityPage(
    root,
    () => 'en',
    installed,
    () => hasGithubToken,
    notify,
  );
  page.show();
  return { root, page, installed, notify };
}
describe('community cards', () => {
  it.each(['.community-name', '.community-preview-button', '.community-install'])(
    'retries failed media when opening details through %s without showing a retry button',
    async (trigger) => {
      const original = mocks.invoke.getMockImplementation()!;
      let requests = 0;
      mocks.invoke.mockImplementation((command: string, args: unknown) => {
        if (command === 'community_media' && requests++ === 0) {
          return Promise.reject(new Error('offline'));
        }
        return original(command, args);
      });
      vi.spyOn(HTMLImageElement.prototype, 'src', 'set').mockImplementation(function (
        this: HTMLImageElement,
      ) {
        this.dispatchEvent(new Event('load'));
      });
      const { root } = page();
      await vi.waitFor(() =>
        expect(root.querySelector('.community-preview')?.textContent).toBe(
          'Screenshot unavailable',
        ),
      );
      expect(
        [...root.querySelectorAll('button')].filter((button) => button.textContent === 'Retry'),
      ).toHaveLength(0);
      root.querySelector<HTMLButtonElement>(trigger)!.click();
      await vi.waitFor(() => expect(root.querySelector('dialog img')).toBeTruthy());
      expect(root.querySelector('article img.preview-image')).toBeTruthy();
      expect(mocks.invoke).toHaveBeenCalledWith('community_media', {
        commit: snapshot.commit,
        package: '@aidget-community/clock',
        path: 'assets/screen.png',
        refresh: true,
      });
      expect(requests).toBe(2);
      expect(
        mocks.invoke.mock.calls.filter(([command]) => command === 'community_list'),
      ).toHaveLength(1);
    },
  );
  it('preserves cards when sorting and refreshing unchanged content', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('article')).toBeTruthy());
    const card = root.querySelector('article');
    root.querySelector<HTMLButtonElement>('[data-sort="published"]')!.click();
    expect(root.querySelector('article')).toBe(card);
    root.querySelector<HTMLButtonElement>('.community-refresh')!.click();
    await vi.waitFor(() => expect(root.getAttribute('aria-busy')).toBe('false'));
    expect(root.querySelector('article')).toBe(card);
  });
  it('distinguishes zero npm downloads from unavailable data', () => {
    expect(weeklyDownloads(0, 'en')).toBe('0');
    expect(weeklyDownloads(null, 'en')).toBeNull();
    expect(weeklyDownloads(16000, 'en')).toBe('16k');
    expect(weeklyDownloads(16000, 'zh')).toBe('16k');
  });
  it('keeps discovery cards compact and shows real zero downloads', async () => {
    const { root } = page();
    await vi.waitFor(() =>
      expect(root.querySelector('.community-name')?.textContent).toBe('Clock'),
    );
    expect(root.querySelector('.community-logo-default')).toBeTruthy();
    const card = root.querySelector('article')!;
    expect(card.textContent).toContain('Clock');
    expect(card.textContent).toContain('v1.0.0');
    expect(card.textContent).not.toContain('A safe desktop clock');
    expect(card.textContent).not.toContain('JavaScript');
    expect(card.textContent).not.toContain('macOS');
    expect(card.textContent).not.toContain('Windows');
    expect(card.textContent).not.toContain('Linux');
    expect(card.textContent).not.toContain('https://api.example.com');
    await vi.waitFor(() => expect(root.textContent).toContain('0/week'));
    expect(root.querySelector('iframe')).toBeNull();
  });
  it('shows refresh progress in the button instead of the status line', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-name')).toBeTruthy());
    let resolveRefresh!: (value: CommunitySnapshot) => void;
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'community_cached') {
        return Promise.resolve(null);
      }
      if (command === 'community_list') {
        return new Promise<CommunitySnapshot>((resolve) => {
          resolveRefresh = resolve;
        });
      }
      return Promise.resolve([]);
    });

    const refresh = root.querySelector<HTMLButtonElement>('.community-refresh')!;
    refresh.click();

    expect(refresh.textContent).toBe('');
    expect(refresh.getAttribute('aria-label')).toBe('Refreshing');
    expect(refresh.querySelector('svg')).toBeTruthy();
    expect(refresh.disabled).toBe(true);
    expect(root.querySelector('.community-status')?.textContent).toBe('');

    resolveRefresh(structuredClone(snapshot));
    await vi.waitFor(() => expect(refresh.getAttribute('aria-label')).toBe('Refresh'));
    expect(refresh.disabled).toBe(false);
  });
  it('requires details confirmation and sends the pinned main SHA', async () => {
    const { root, installed, notify } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const dialog = root.querySelector('dialog')!;
    expect(dialog.open).toBe(true);
    expect(dialog.textContent).toContain('A safe desktop clock');
    expect(dialog.textContent).toContain('JavaScript');
    expect(dialog.textContent).toContain('https://api.example.com');
    expect(mocks.invoke).not.toHaveBeenCalledWith('community_install', expect.anything());
    (root.querySelector('dialog .community-install') as HTMLButtonElement).click();
    await vi.waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('community_install', {
        commit: 'a'.repeat(40),
        package: '@aidget-community/clock',
      }),
    );
    await vi.waitFor(() => expect(installed).toHaveBeenCalled());
    expect(notify).toHaveBeenCalledWith('Installed in Local Widgets');
    expect(root.querySelector('.community-install')?.textContent).toBe('Installed');
  });
  it('pins install in the dialog header and close as a text action in the footer', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    const card = root.querySelector('article')!;
    expect(card.firstElementChild?.classList.contains('community-card-heading')).toBe(true);

    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const dialog = root.querySelector('dialog')!;
    const header = dialog.querySelector('.community-dialog-header')!;
    const content = dialog.querySelector('.community-dialog-content')!;
    const footer = dialog.querySelector('.community-dialog-footer')!;
    expect(dialog.firstElementChild).toBe(header);
    expect(dialog.lastElementChild).toBe(footer);
    expect(header.querySelector('h2')?.textContent).toBe('Clock');
    expect(header.querySelector('.community-install')).toBeTruthy();
    expect(content.querySelector('.community-install')).toBeNull();
    expect(footer.querySelector('.community-install')).toBeNull();
    const close = footer.querySelector<HTMLButtonElement>('.community-dialog-close')!;
    expect(close.textContent).toBe('Close');
    expect(close.classList.contains('secondary-button')).toBe(false);
    expect(
      [...dialog.querySelectorAll('.community-platforms li')].map((item) => item.textContent),
    ).toEqual(['macOS', 'Linux', 'Windows']);
    expect(dialog.textContent).not.toContain('360 × 220');
  });
  it('focuses the dialog itself instead of the close action', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const dialog = root.querySelector('dialog')!;
    const close = dialog.querySelector('.community-dialog-close');
    expect(dialog.tabIndex).toBe(-1);
    expect(document.activeElement).toBe(dialog);
    expect(document.activeElement).not.toBe(close);
  });
  it('closes the dialog from the footer text action', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const dialog = root.querySelector('dialog')!;
    expect(dialog.open).toBe(true);
    (dialog.querySelector('.community-dialog-close') as HTMLButtonElement).click();
    expect(dialog.open).toBe(false);
  });
  it('places the detail description after the screenshot gallery', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const images = root.querySelector('dialog .community-detail-images')!;
    const description = root.querySelector('dialog .community-detail-description');
    expect(description).toBeTruthy();
    expect(images.nextElementSibling).toBe(description);
  });
  it('shows empty permissions and network access in the confirmation only', async () => {
    const withoutPermissions = structuredClone(snapshot);
    withoutPermissions.widgets[0].entry.config.permissions = [];
    withoutPermissions.widgets[0].entry.config.network = [];
    mocks.invoke.mockImplementation((command: string) =>
      command === 'community_cached'
        ? Promise.resolve(null)
        : command === 'community_list'
          ? Promise.resolve(withoutPermissions)
          : Promise.resolve([]),
    );
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-name')).toBeTruthy());
    expect(root.querySelector('article')?.textContent).not.toContain('No special permissions');
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const dialog = root.querySelector('dialog')!;
    expect(dialog.textContent).toContain('No special permissions');
    expect(dialog.querySelector('ul.community-permissions')).toBeNull();
    expect(dialog.querySelector('.community-permissions-empty')?.tagName).toBe('P');
    expect(dialog.querySelector('ul.community-network')).toBeNull();
    const network = dialog.querySelector('.community-network-empty')!;
    expect(network.tagName).toBe('P');
    expect(network.textContent).toBe('No network access');
  });
  it('renders actual network origins in a list', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const network = root.querySelector('dialog ul.community-network')!;
    expect(network).toBeTruthy();
    expect(network.textContent).toContain('https://api.example.com');
    expect(root.querySelector('dialog .community-network-empty')).toBeNull();
  });
  it('pins an open confirmation dialog even when the catalogue refreshes', async () => {
    const { root, page: controller } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    const newer = structuredClone(snapshot);
    newer.commit = 'b'.repeat(40);
    newer.widgets[0].entry.config.network = ['https://changed.example.com'];
    mocks.invoke.mockImplementation((command: string) =>
      command === 'community_cached'
        ? Promise.resolve(null)
        : command === 'community_list'
          ? Promise.resolve(newer)
          : Promise.resolve([]),
    );
    controller.invalidate(true);
    controller.show();
    await vi.waitFor(() =>
      expect(
        mocks.invoke.mock.calls.filter(([command]) => command === 'community_list'),
      ).toHaveLength(2),
    );
    await vi.waitFor(() =>
      expect((root.querySelector('dialog .community-install') as HTMLButtonElement).disabled).toBe(
        false,
      ),
    );
    expect(root.querySelector('dialog')?.textContent).toContain('https://api.example.com');
    expect(root.querySelector('dialog')?.textContent).not.toContain('https://changed.example.com');
    (root.querySelector('dialog .community-install') as HTMLButtonElement).click();
    await vi.waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('community_install', {
        commit: 'a'.repeat(40),
        package: '@aidget-community/clock',
      }),
    );
  });
  it('keeps installation failures visible and permits retry', async () => {
    mocks.invoke.mockImplementation((command: string) =>
      command === 'community_cached'
        ? Promise.resolve(null)
        : command === 'community_list'
          ? Promise.resolve(structuredClone(snapshot))
          : command === 'community_install'
            ? Promise.reject(new Error('integrity mismatch'))
            : Promise.resolve([]),
    );
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('.community-install')).toBeTruthy());
    (root.querySelector('.community-install') as HTMLButtonElement).click();
    (root.querySelector('dialog .community-install') as HTMLButtonElement).click();
    await vi.waitFor(() =>
      expect(root.querySelector('.community-install-error')?.textContent).toContain(
        'integrity mismatch',
      ),
    );
    expect((root.querySelector('dialog .community-install') as HTMLButtonElement).disabled).toBe(
      false,
    );
  });
  it('disables unsupported platforms and does not fabricate download counts', async () => {
    const other = structuredClone(snapshot);
    other.widgets[0].supported = false;
    mocks.invoke.mockImplementation((command: string) =>
      command === 'community_cached'
        ? Promise.resolve(null)
        : command === 'community_list'
          ? Promise.resolve(other)
          : Promise.resolve([]),
    );
    const { root } = page();
    await vi.waitFor(() =>
      expect(root.querySelector('.community-install')?.textContent).toBe('Incompatible'),
    );
    expect((root.querySelector('.community-install') as HTMLButtonElement).disabled).toBe(true);
    expect(root.querySelector<HTMLElement>('[data-download-count]')?.hidden).toBe(true);
  });
  it('suggests configuring a GitHub token when an unauthenticated load fails', async () => {
    mocks.invoke.mockImplementation((command: string) =>
      command === 'community_cached'
        ? Promise.resolve(null)
        : Promise.reject(new Error('HTTP 403 Forbidden')),
    );
    const { root } = page(false);

    await vi.waitFor(() => {
      expect(root.querySelector('.community-status')?.textContent).toContain(
        'configure a GitHub token',
      );
    });
  });
  it('does not suggest a GitHub token when community loading fails with one configured', async () => {
    mocks.invoke.mockImplementation((command: string) =>
      command === 'community_cached'
        ? Promise.resolve(null)
        : Promise.reject(new Error('HTTP 403 Forbidden')),
    );
    const { root } = page(true);

    await vi.waitFor(() => {
      expect(root.querySelector('.community-status')?.textContent).toContain('HTTP 403 Forbidden');
    });
    expect(root.querySelector('.community-status')?.textContent).not.toContain(
      'configure a GitHub token',
    );
  });
});

describe('community discovery controls', () => {
  it('hides refresh and announces first loading, then exposes retry after failure', async () => {
    let reject!: (error: Error) => void;
    mocks.invoke.mockReturnValue(
      new Promise((_, fail) => {
        reject = fail;
      }),
    );
    const { root } = page();
    const refresh = root.querySelector<HTMLButtonElement>('.community-refresh')!;
    const status = root.querySelector('.community-status')!;
    expect(refresh.hidden).toBe(true);
    expect(status.textContent).toBe('Fetching data...');
    expect(root.querySelector('.community-toolbar')?.nextElementSibling).toBe(status);
    expect(status.nextElementSibling).toBe(root.querySelector('.widget-grid'));
    expect(status.closest('.community-heading')).toBeNull();
    expect(status.getAttribute('role')).toBe('status');
    reject(new Error('offline'));
    await vi.waitFor(() => expect(status.textContent).toContain('offline'));
    expect(refresh.hidden).toBe(false);
    expect(refresh.disabled).toBe(false);
    mocks.invoke.mockImplementation((command: string) =>
      Promise.resolve(
        command === 'community_cached'
          ? null
          : command === 'community_list'
            ? structuredClone(snapshot)
            : [],
      ),
    );
    refresh.click();
    expect(refresh.hidden).toBe(true);
    await vi.waitFor(() => expect(root.querySelector('article')).toBeTruthy());
    expect(refresh.hidden).toBe(false);
    expect(status.textContent).toBe('');
  });

  it('renders and translates the banner and opens its repository link', async () => {
    const root = document.createElement('section');
    document.body.append(root);
    let locale: 'en' | 'zh-CN' = 'zh-CN';
    const controller = createCommunityPage(
      root,
      () => locale,
      async () => {},
      () => false,
      vi.fn(),
    );
    controller.show();
    expect(root.querySelector('.community-status')?.textContent).toBe('正在拉取数据...');
    const banner = root.querySelector('.community-banner')!;
    expect(banner.textContent).toBe(
      '这里是由 aidget 社区经过安全评审的小组件，你可前往 组件仓库 了解如何发布社区组件。',
    );
    const link = banner.querySelector<HTMLAnchorElement>('a')!;
    expect(link.href).toBe('https://github.com/aidget-ai/widgets');
    expect(link.lastElementChild?.tagName.toLowerCase()).toBe('svg');
    link.click();
    await vi.waitFor(() => expect(mocks.openUrl).toHaveBeenCalledWith(link.href));
    locale = 'en';
    controller.render();
    expect(link.textContent).toBe('widget repository');
    expect(banner.textContent).toContain('security-reviewed');
    expect(root.querySelector('.community-sort')?.getAttribute('aria-label')).toBe('Sort');
    expect(root.querySelectorAll('.community-sort button')).toHaveLength(2);
  });

  it('places the banner above the title and remembers dismissal across page instances', () => {
    const { root, page: controller } = page();
    const banner = root.querySelector<HTMLElement>('.community-banner')!;
    expect(root.firstElementChild).toBe(banner);
    expect(banner.nextElementSibling?.className).toBe('community-heading');
    const close = banner.querySelector<HTMLButtonElement>('button')!;
    expect(close.getAttribute('aria-label')).toBe('Dismiss community introduction');
    expect(banner.hidden).toBe(false);
    close.click();
    expect(banner.hidden).toBe(true);
    expect(localStorage.getItem('aidget.communityBannerDismissed')).toBe('true');
    controller.render();
    expect(banner.hidden).toBe(true);
    const next = page();
    expect(next.root.querySelector<HTMLElement>('.community-banner')!.hidden).toBe(true);
    localStorage.removeItem('aidget.communityBannerDismissed');
    expect(page().root.querySelector<HTMLElement>('.community-banner')!.hidden).toBe(false);
  });

  it('exposes the sorting group beside refresh and switches metric and direction', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('article')).toBeTruthy());
    const group = root.querySelector<HTMLElement>('.community-sort')!;
    const refresh = root.querySelector<HTMLButtonElement>('.community-refresh')!;
    expect(group.nextElementSibling).toBe(refresh);
    expect(root.querySelector('[role="menu"]')).toBeNull();
    const rows = [...group.querySelectorAll<HTMLButtonElement>('button')];
    expect(rows.map((row) => row.textContent)).toEqual(['Published', 'Weekly downloads']);
    expect(rows[0].dataset.direction).toBe('desc');
    rows[0].click();
    expect(rows[0].dataset.direction).toBe('asc');
    rows[1].click();
    expect(rows[1].dataset.direction).toBe('desc');
    expect(rows[0].getAttribute('aria-pressed')).toBe('false');
    expect(rows[1].getAttribute('aria-pressed')).toBe('true');
  });

  it('reports a repository open failure without losing the catalogue', async () => {
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('article')).toBeTruthy());
    mocks.openUrl.mockRejectedValueOnce(new Error('browser unavailable'));
    root.querySelector<HTMLAnchorElement>('.community-banner a')!.click();
    await vi.waitFor(() =>
      expect(root.querySelector('.community-status')?.textContent).toContain('browser unavailable'),
    );
    expect(root.querySelector('article')).toBeTruthy();
  });

  it('sorts by both metrics in both directions and retains selection through lifecycle changes', async () => {
    const entries = ['alpha', 'beta', 'gamma', 'delta', 'epsilon'].map((id, index) => {
      const entry = structuredClone(snapshot.widgets[0]);
      entry.entry.config.id = id;
      entry.entry.config.title = id;
      entry.entry.package.name = `@aidget-community/${id}`;
      entry.entry.publishedAt = `2026-10-${String(25 + (index % 3)).padStart(2, '0')}T00:00:00Z`;
      return entry;
    });
    const catalogue: CommunitySnapshot = { commit: snapshot.commit, widgets: entries };
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'community_cached') {
        return Promise.resolve(null);
      }
      if (command === 'community_list') {
        return Promise.resolve(catalogue);
      }
      if (command === 'community_downloads') {
        return Promise.resolve(
          entries.map((entry, i) => ({
            package: entry.entry.package.name,
            weekly: [0, 50, null, 50, null][i],
            error: null,
          })),
        );
      }
      return Promise.resolve();
    });
    const { root, page: controller, installed } = page();
    await vi.waitFor(() => expect(root.querySelectorAll('article')).toHaveLength(5));
    const group = root.querySelector<HTMLElement>('.community-sort')!;
    const selectedSort = () => {
      const active = group.querySelector<HTMLElement>('[aria-pressed="true"]')!;
      return `${active.dataset.sort}-${active.dataset.direction}`;
    };
    expect(selectedSort()).toBe('published-desc');
    const order = () =>
      [...root.querySelectorAll<HTMLElement>('article')].map(
        (card) => card.dataset.package!.split('/')[1],
      );
    const choose = (value: string) => {
      const [metric, direction] = value.split('-');
      const item = group.querySelector<HTMLButtonElement>(`[data-sort="${metric}"]`)!;
      item.click();
      if (item.dataset.direction !== direction) {
        item.click();
      }
    };
    choose('published-desc');
    expect(order()).toEqual(['gamma', 'beta', 'epsilon', 'alpha', 'delta']);
    choose('downloads-desc');
    expect(order()).toEqual(['beta', 'delta', 'alpha', 'epsilon', 'gamma']);
    choose('downloads-asc');
    expect(order()).toEqual(['alpha', 'beta', 'delta', 'epsilon', 'gamma']);
    choose('published-asc');
    expect(order()).toEqual(['alpha', 'delta', 'beta', 'epsilon', 'gamma']);
    expect(catalogue.widgets.map((e) => e.entry.config.id)).toEqual([
      'alpha',
      'beta',
      'gamma',
      'delta',
      'epsilon',
    ]);
    controller.show();
    expect(selectedSort()).toBe('published-asc');
    root.querySelector<HTMLButtonElement>('.community-install')!.click();
    root.querySelector<HTMLButtonElement>('dialog .community-install')!.click();
    await vi.waitFor(() => expect(installed).toHaveBeenCalled());
    expect(selectedSort()).toBe('published-asc');
    root.querySelector<HTMLButtonElement>('.community-refresh')!.click();
    expect(order()).toEqual(['alpha', 'delta', 'beta', 'epsilon', 'gamma']);
    await vi.waitFor(() => expect(root.getAttribute('aria-busy')).toBe('false'));
    expect(selectedSort()).toBe('published-asc');
    controller.invalidate();
    controller.show();
    await vi.waitFor(() => expect(root.getAttribute('aria-busy')).toBe('false'));
    expect(selectedSort()).toBe('published-asc');
  });

  it('preserves the selected order and existing cards until refreshed counts arrive', async () => {
    const catalogue = structuredClone(snapshot);
    const second = structuredClone(catalogue.widgets[0]);
    second.entry.package.name = '@aidget-community/zebra';
    catalogue.widgets.push(second);
    const values = catalogue.widgets.map((entry, index) => ({
      package: entry.entry.package.name,
      weekly: index * 10,
      error: null,
    }));
    let resolveCounts!: (counts: DownloadCount[]) => void;
    let downloadsRequested = 0;
    let onCount!: Channel<DownloadCount>;
    mocks.invoke.mockImplementation(
      (command: string, args?: { onCount: Channel<DownloadCount> }) => {
        if (command === 'community_cached') {
          return Promise.resolve(null);
        }
        if (command === 'community_list') {
          return Promise.resolve(catalogue);
        }
        if (command === 'community_downloads') {
          onCount = args!.onCount;
          downloadsRequested++;
          return downloadsRequested === 1
            ? Promise.resolve(values)
            : new Promise<DownloadCount[]>((resolve) => {
                resolveCounts = resolve;
              });
        }
        return Promise.resolve();
      },
    );
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelectorAll('article')).toHaveLength(2));
    const downloads = root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!;
    await vi.waitFor(() => expect(downloads.disabled).toBe(false));
    downloads.click();
    downloads.click();
    const cards = [...root.querySelectorAll('article')];
    expect(cards[0].getAttribute('data-package')).toBe('@aidget-community/clock');
    root.querySelector<HTMLButtonElement>('.community-refresh')!.click();
    await vi.waitFor(() => expect(resolveCounts).toBeTypeOf('function'));
    expect(root.getAttribute('aria-busy')).toBe('true');
    expect(downloads.dataset.direction).toBe('asc');
    expect([...root.querySelectorAll('article')].map((card) => card.dataset.package)).toEqual(
      cards.map((card) => card.dataset.package),
    );
    expect(root.textContent).toContain('10/week');
    const waitingCards = [...root.querySelectorAll('article')];
    onCount.onmessage({ ...values[0], weekly: 10 });
    expect(root.querySelector('[data-download-count="@aidget-community/clock"]')?.textContent).toBe(
      '↓ 10/week',
    );
    expect([...root.querySelectorAll('article')]).toEqual(waitingCards);
    expect(downloads.disabled).toBe(true);
    resolveCounts(values.map((value) => ({ ...value, weekly: 10 - value.weekly })));
    await vi.waitFor(() =>
      expect(root.querySelector<HTMLElement>('article')?.dataset.package).toBe(
        second.entry.package.name,
      ),
    );
    expect(downloads.dataset.direction).toBe('asc');
  });

  it('opens kept cards from the displayed revision while refreshed counts are pending', async () => {
    const older = structuredClone(snapshot);
    const newer = structuredClone(older);
    newer.commit = 'b'.repeat(40);
    newer.widgets[0].entry.config.network = ['https://changed.example.com'];
    let lists = 0;
    let downloads = 0;
    let resolveCounts!: (counts: DownloadCount[]) => void;
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'community_cached') {
        return Promise.resolve(null);
      }
      if (command === 'community_list') {
        return Promise.resolve(lists++ === 0 ? older : newer);
      }
      if (command === 'community_downloads') {
        return downloads++ === 0
          ? Promise.resolve([{ package: '@aidget-community/clock', weekly: 0, error: null }])
          : new Promise<DownloadCount[]>((resolve) => {
              resolveCounts = resolve;
            });
      }
      return Promise.resolve();
    });
    const { root } = page();
    await vi.waitFor(() =>
      expect(root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!.disabled).toBe(
        false,
      ),
    );
    root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!.click();
    const kept = root.querySelector('article')!;
    root.querySelector<HTMLButtonElement>('.community-refresh')!.click();
    await vi.waitFor(() => expect(resolveCounts).toBeTypeOf('function'));
    expect(root.querySelector('article')).toBe(kept);

    kept.querySelector<HTMLButtonElement>('.community-install')!.click();

    const dialog = root.querySelector('dialog')!;
    expect(dialog.textContent).toContain('https://api.example.com');
    expect(dialog.textContent).not.toContain('https://changed.example.com');
    dialog.querySelector<HTMLButtonElement>('.community-install')!.click();
    await vi.waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith('community_install', {
        commit: older.commit,
        package: '@aidget-community/clock',
      }),
    );
  });

  it('shows the catalogue when download statistics fail and finishes loading', async () => {
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'community_cached') {
        return Promise.resolve(null);
      }
      if (command === 'community_list') {
        return Promise.resolve(structuredClone(snapshot));
      }
      if (command === 'community_downloads') {
        return Promise.reject(new Error('statistics offline'));
      }
      return Promise.resolve();
    });
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelector('article')).toBeTruthy());
    expect(root.getAttribute('aria-busy')).toBe('false');
    expect(root.querySelector('.community-status')?.getAttribute('title')).toBeNull();
    expect(root.querySelector('.community-status')?.textContent).toBe('');
    expect(root.querySelector<HTMLElement>('[data-download-count]')?.hidden).toBe(true);
    expect(root.querySelector<HTMLButtonElement>('.community-refresh')?.disabled).toBe(false);
  });

  it.each(['success', 'failure'])(
    'shows published cards while disabling downloads until statistics %s',
    async (outcome) => {
      const catalogue = structuredClone(snapshot);
      const second = structuredClone(catalogue.widgets[0]);
      second.entry.package.name = '@aidget-community/zebra';
      second.entry.publishedAt = '2026-10-24T00:00:00Z';
      catalogue.widgets.push(second);
      let resolveCounts!: (counts: DownloadCount[]) => void;
      let rejectCounts!: (error: Error) => void;
      mocks.invoke.mockImplementation((command: string) => {
        if (command === 'community_cached') {
          return Promise.resolve(null);
        }
        if (command === 'community_list') {
          return Promise.resolve(catalogue);
        }
        if (command === 'community_downloads') {
          return new Promise<DownloadCount[]>((resolve, reject) => {
            resolveCounts = resolve;
            rejectCounts = reject;
          });
        }
        return Promise.resolve();
      });
      const { root } = page();
      const published = root.querySelector<HTMLButtonElement>('[data-sort="published"]')!;
      const downloads = root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!;
      expect(published.disabled).toBe(true);
      expect(downloads.disabled).toBe(true);
      await vi.waitFor(() => expect(resolveCounts).toBeTypeOf('function'));
      expect(root.querySelectorAll('article')).toHaveLength(2);
      expect(root.getAttribute('aria-busy')).toBe('false');
      expect(published.disabled).toBe(false);
      expect(downloads.disabled).toBe(true);
      expect(published.dataset.direction).toBe('desc');
      expect(root.querySelector<HTMLElement>('article')?.dataset.package).toBe(
        '@aidget-community/clock',
      );
      published.click();
      expect(published.dataset.direction).toBe('asc');
      expect(downloads.disabled).toBe(true);
      expect(root.querySelector<HTMLElement>('article')?.dataset.package).toBe(
        second.entry.package.name,
      );
      if (outcome === 'success') {
        resolveCounts([{ package: second.entry.package.name, weekly: 10, error: null }]);
      } else {
        rejectCounts(new Error('statistics offline'));
      }
      await vi.waitFor(() => expect(downloads.disabled).toBe(false));
      expect(published.dataset.direction).toBe('asc');
      expect(root.querySelector<HTMLElement>('article')?.dataset.package).toBe(
        second.entry.package.name,
      );
      if (outcome === 'success') {
        expect(root.textContent).toContain('10/week');
      } else {
        expect(root.querySelector<HTMLElement>('[data-download-count]')?.hidden).toBe(true);
      }
    },
  );

  it('allows switching to published order while refreshed download counts are pending', async () => {
    let resolveCounts!: (counts: DownloadCount[]) => void;
    const { root } = page();
    const downloads = root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!;
    const published = root.querySelector<HTMLButtonElement>('[data-sort="published"]')!;
    await vi.waitFor(() => expect(downloads.disabled).toBe(false));
    downloads.click();
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'community_cached') {
        return Promise.resolve(null);
      }
      if (command === 'community_list') {
        return Promise.resolve(structuredClone(snapshot));
      }
      if (command === 'community_downloads') {
        return new Promise<DownloadCount[]>((resolve) => {
          resolveCounts = resolve;
        });
      }
      return Promise.resolve();
    });
    root.querySelector<HTMLButtonElement>('.community-refresh')!.click();
    await vi.waitFor(() => expect(resolveCounts).toBeTypeOf('function'));
    expect(root.getAttribute('aria-busy')).toBe('true');
    expect(downloads.disabled).toBe(true);
    expect(published.disabled).toBe(false);
    published.click();
    expect(root.getAttribute('aria-busy')).toBe('false');
    expect(root.querySelectorAll('article')).toHaveLength(1);
    expect(published.dataset.direction).toBe('desc');
    expect(downloads.disabled).toBe(true);
    resolveCounts([]);
    await vi.waitFor(() => expect(downloads.disabled).toBe(false));
    expect(published.dataset.direction).toBe('desc');
  });

  it.each(['resolve', 'reject'])(
    'ignores stale statistics that %s during a later refresh',
    async (outcome) => {
      const pending: {
        resolve: (counts: DownloadCount[]) => void;
        reject: (error: Error) => void;
      }[] = [];
      mocks.invoke.mockImplementation((command: string) => {
        if (command === 'community_cached') {
          return Promise.resolve(null);
        }
        if (command === 'community_list') {
          return Promise.resolve(structuredClone(snapshot));
        }
        if (command === 'community_downloads') {
          return new Promise<DownloadCount[]>((resolve, reject) => {
            pending.push({ resolve, reject });
          });
        }
        return Promise.resolve();
      });
      const { root } = page();
      await vi.waitFor(() => expect(pending).toHaveLength(1));
      root.querySelector<HTMLButtonElement>('.community-refresh')!.click();
      await vi.waitFor(() => expect(pending).toHaveLength(2));
      if (outcome === 'resolve') {
        pending[0].resolve([{ package: '@aidget-community/clock', weekly: 999, error: null }]);
      } else {
        pending[0].reject(new Error('stale request'));
      }
      await new Promise((resolve) => setTimeout(resolve, 0));
      const downloads = root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!;
      expect(downloads.disabled).toBe(true);
      expect(root.textContent).not.toContain('999/week');
      pending[1].resolve([{ package: '@aidget-community/clock', weekly: 12, error: null }]);
      await vi.waitFor(() => expect(downloads.disabled).toBe(false));
      expect(root.textContent).toContain('12/week');
      expect(root.querySelector<HTMLElement>('[data-sort="published"]')?.dataset.direction).toBe(
        'desc',
      );
    },
  );
  it('updates each count immediately without replacing or reordering cards', async () => {
    const catalogue = structuredClone(snapshot);
    for (const name of ['zero', 'missing']) {
      const entry = structuredClone(catalogue.widgets[0]);
      entry.entry.package.name = `@aidget-community/${name}`;
      catalogue.widgets.push(entry);
    }
    let channel!: Channel<DownloadCount>;
    let finish!: (values: DownloadCount[]) => void;
    mocks.invoke.mockImplementation(
      (command: string, args?: { onCount: Channel<DownloadCount> }) => {
        if (command === 'community_cached') {
          return Promise.resolve(null);
        }
        if (command === 'community_list') {
          return Promise.resolve(catalogue);
        }
        if (command === 'community_downloads') {
          channel = args!.onCount;
          return new Promise<DownloadCount[]>((resolve) => {
            finish = resolve;
          });
        }
        return Promise.resolve();
      },
    );
    const { root } = page();
    await vi.waitFor(() => expect(finish).toBeTypeOf('function'));
    expect(channel).toBeDefined();
    const cards = [...root.querySelectorAll('article')];
    const title = cards[0].querySelector<HTMLButtonElement>('.community-name')!;
    title.focus();
    const values: DownloadCount[] = [
      { package: '@aidget-community/clock', weekly: 42, error: null },
      { package: '@aidget-community/zero', weekly: 0, error: null },
      { package: '@aidget-community/missing', weekly: null, error: 'unavailable' },
    ];
    const count = (name: string) =>
      root.querySelector<HTMLElement>(`[data-download-count="@aidget-community/${name}"]`)!;
    channel.onmessage(values[1]);
    expect(count('zero').textContent).toBe('↓ 0/week');
    expect(count('zero').hidden).toBe(false);
    expect(count('clock').hidden).toBe(true);
    channel.onmessage(values[0]);
    expect(count('clock').textContent).toBe('↓ 42/week');
    channel.onmessage(values[2]);
    expect(count('missing').hidden).toBe(true);
    expect([...root.querySelectorAll('article')]).toEqual(cards);
    expect(document.activeElement).toBe(title);
    expect(root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!.disabled).toBe(true);
    finish(values);
    await vi.waitFor(() =>
      expect(root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!.disabled).toBe(
        false,
      ),
    );
    expect(count('clock').textContent).toBe('↓ 42/week');
  });

  it('ignores progress from earlier requests and from a completed channel', async () => {
    const channels: Channel<DownloadCount>[] = [];
    const finish: ((values: DownloadCount[]) => void)[] = [];
    mocks.invoke.mockImplementation(
      (command: string, args?: { onCount: Channel<DownloadCount> }) => {
        if (command === 'community_cached') {
          return Promise.resolve(null);
        }
        if (command === 'community_list') {
          return Promise.resolve(structuredClone(snapshot));
        }
        if (command === 'community_downloads') {
          channels.push(args!.onCount);
          return new Promise<DownloadCount[]>((resolve) => {
            finish.push(resolve);
          });
        }
        return Promise.resolve();
      },
    );
    const { root } = page();
    await vi.waitFor(() => expect(channels).toHaveLength(1));
    expect(channels[0]).toBeDefined();
    root.querySelector<HTMLButtonElement>('.community-refresh')!.click();
    await vi.waitFor(() => expect(channels).toHaveLength(2));
    const fresh: DownloadCount = { package: '@aidget-community/clock', weekly: 12, error: null };
    const stale: DownloadCount = { ...fresh, weekly: 999 };
    channels[1].onmessage(fresh);
    channels[0].onmessage(stale);
    expect(root.textContent).toContain('12/week');
    expect(root.textContent).not.toContain('999/week');
    expect(root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!.disabled).toBe(true);
    finish[1]([fresh]);
    await vi.waitFor(() =>
      expect(root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!.disabled).toBe(
        false,
      ),
    );
    channels[1].onmessage(stale);
    finish[0]([stale]);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(root.textContent).toContain('12/week');
    expect(root.textContent).not.toContain('999/week');
  });
});

describe('persistent catalogue browsing', () => {
  it('shows cached content before a delayed update and retains cards and images across unchanged refreshes', async () => {
    const original = mocks.invoke.getMockImplementation()!;
    let resolve!: (value: CommunitySnapshot) => void;
    mocks.invoke.mockImplementation((command: string, args: unknown) => {
      if (command === 'community_cached') {
        return Promise.resolve({ snapshot: structuredClone(snapshot), checkedAt: 1 });
      }
      if (command === 'community_list') {
        return new Promise<CommunitySnapshot>((done) => {
          resolve = done;
        });
      }
      return original(command, args);
    });
    const { root } = page();
    await vi.waitFor(() => expect(resolve).toBeTypeOf('function'));
    const card = root.querySelector('article')!;
    expect(card).toBeTruthy();
    expect(root.querySelector<HTMLButtonElement>('.community-install')!.disabled).toBe(false);
    expect(root.querySelector<HTMLButtonElement>('[data-sort="published"]')!.disabled).toBe(false);
    expect(root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!.disabled).toBe(false);
    const name = card.querySelector<HTMLButtonElement>('.community-name')!;
    name.focus();
    resolve(structuredClone(snapshot));
    await vi.waitFor(() => expect(root.getAttribute('aria-busy')).toBe('false'));
    expect(root.querySelector('article')).toBe(card);
    expect(document.activeElement).toBe(name);
  });

  it('throttles successful and failed entry checks for five minutes while manual refresh always checks', async () => {
    const clock = vi.spyOn(Date, 'now').mockReturnValue(1_000_000);
    const original = mocks.invoke.getMockImplementation()!;
    let checks = 0;
    mocks.invoke.mockImplementation((command: string, args: unknown) => {
      if (command === 'community_cached') {
        return Promise.resolve({ snapshot: structuredClone(snapshot), checkedAt: 1000 });
      }
      if (command === 'community_list') {
        checks++;
        return Promise.reject(new Error('offline'));
      }
      return original(command, args);
    });
    try {
      const { root, page: controller } = page();
      await vi.waitFor(() => expect(root.querySelector('article')).toBeTruthy());
      expect(checks).toBe(0);
      clock.mockReturnValue(1_300_000);
      controller.show();
      expect(checks).toBe(0);
      controller.hide();
      controller.show();
      await vi.waitFor(() => expect(checks).toBe(1));
      await vi.waitFor(() => expect(root.getAttribute('aria-busy')).toBe('false'));
      expect(root.querySelector('.community-status')!.textContent).toContain(
        'Showing cached content',
      );
      expect(root.querySelector('.community-status')!.textContent).toContain('Last updated');
      controller.hide();
      controller.show();
      expect(checks).toBe(1);
      root.querySelector<HTMLButtonElement>('.community-refresh')!.click();
      await vi.waitFor(() => expect(checks).toBe(2));
      await vi.waitFor(() => expect(root.getAttribute('aria-busy')).toBe('false'));
      root.querySelector<HTMLButtonElement>('.community-install')!.click();
      root.querySelector<HTMLButtonElement>('dialog .community-install')!.click();
      await vi.waitFor(() =>
        expect(mocks.invoke).toHaveBeenCalledWith('community_install', {
          commit: snapshot.commit,
          package: '@aidget-community/clock',
        }),
      );
    } finally {
      clock.mockRestore();
    }
  });

  it('recomputes local availability without checking main and checks immediately after credential changes', async () => {
    const original = mocks.invoke.getMockImplementation()!;
    const cached = {
      snapshot: structuredClone(snapshot),
      checkedAt: Math.floor(Date.now() / 1000),
    };
    mocks.invoke.mockImplementation((command: string, args: unknown) =>
      command === 'community_cached'
        ? Promise.resolve(structuredClone(cached))
        : original(command, args),
    );
    const { root, page: controller } = page();
    await vi.waitFor(() => expect(root.querySelector('article')).toBeTruthy());
    cached.snapshot.widgets[0].installed = true;
    controller.invalidate();
    await vi.waitFor(() =>
      expect(root.querySelector('.community-install')!.textContent).toBe('Installed'),
    );
    expect(
      mocks.invoke.mock.calls.filter(([command]) => command === 'community_list'),
    ).toHaveLength(0);
    controller.invalidate(true);
    await vi.waitFor(() =>
      expect(
        mocks.invoke.mock.calls.filter(([command]) => command === 'community_list'),
      ).toHaveLength(1),
    );
  });

  it('ignores an older disk response after credentials trigger a newer restore', async () => {
    const original = mocks.invoke.getMockImplementation()!;
    const pending: ((value: { snapshot: CommunitySnapshot; checkedAt: number }) => void)[] = [];
    mocks.invoke.mockImplementation((command: string, args: unknown) =>
      command === 'community_cached'
        ? new Promise((resolve) => pending.push(resolve))
        : original(command, args),
    );
    const { root, page: controller } = page();
    controller.invalidate(true);
    expect(pending).toHaveLength(2);
    pending[1]({ snapshot: structuredClone(snapshot), checkedAt: Math.floor(Date.now() / 1000) });
    await vi.waitFor(() =>
      expect(root.querySelector('.community-name')!.textContent).toBe('Clock'),
    );
    const old = structuredClone(snapshot);
    old.widgets[0].entry.config.title = 'Obsolete title';
    pending[0]({ snapshot: old, checkedAt: 1 });
    await Promise.resolve();
    expect(root.querySelector('.community-name')!.textContent).toBe('Clock');
  });
});
