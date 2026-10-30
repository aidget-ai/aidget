// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vite-plus/test';

import type { CommunitySnapshot, DownloadCount } from '../../src/types/rust';
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), openUrl: vi.fn() }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: mocks.openUrl }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
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
        files: [],
      },
      installed: false,
      idConflict: false,
      supported: true,
    },
  ],
};
beforeEach(() => {
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
  const page = createCommunityPage(
    root,
    () => 'en',
    installed,
    () => hasGithubToken,
  );
  page.show();
  return { root, page, installed };
}
describe('community cards', () => {
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
    const { root, installed } = page();
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
      command === 'community_list' ? Promise.resolve(withoutPermissions) : Promise.resolve([]),
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
      command === 'community_list' ? Promise.resolve(newer) : Promise.resolve([]),
    );
    controller.invalidate();
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
      command === 'community_list'
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
      command === 'community_list' ? Promise.resolve(other) : Promise.resolve([]),
    );
    const { root } = page();
    await vi.waitFor(() =>
      expect(root.querySelector('.community-install')?.textContent).toBe('Incompatible'),
    );
    expect((root.querySelector('.community-install') as HTMLButtonElement).disabled).toBe(true);
    expect(root.querySelector<HTMLElement>('[data-download-count]')?.hidden).toBe(true);
  });
  it('suggests configuring a GitHub token when an unauthenticated load fails', async () => {
    mocks.invoke.mockRejectedValue(new Error('HTTP 403 Forbidden'));
    const { root } = page(false);

    await vi.waitFor(() => {
      expect(root.querySelector('.community-status')?.textContent).toContain(
        'configure a GitHub token',
      );
    });
  });
  it('does not suggest a GitHub token when community loading fails with one configured', async () => {
    mocks.invoke.mockRejectedValue(new Error('HTTP 403 Forbidden'));
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
      Promise.resolve(command === 'community_list' ? structuredClone(snapshot) : []),
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
    expect(rows.map((row) => row.textContent)).toEqual(['Weekly downloads', 'Published']);
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
    expect(selectedSort()).toBe('downloads-desc');
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
    mocks.invoke.mockImplementation((command: string) => {
      if (command === 'community_list') {
        return Promise.resolve(catalogue);
      }
      if (command === 'community_downloads') {
        downloadsRequested++;
        return downloadsRequested === 1
          ? Promise.resolve(values)
          : new Promise<DownloadCount[]>((resolve) => {
              resolveCounts = resolve;
            });
      }
      return Promise.resolve();
    });
    const { root } = page();
    await vi.waitFor(() => expect(root.querySelectorAll('article')).toHaveLength(2));
    const downloads = root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!;
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
    resolveCounts(values.map((value) => ({ ...value, weekly: 10 - value.weekly })));
    await vi.waitFor(() =>
      expect(root.querySelector<HTMLElement>('article')?.dataset.package).toBe(
        second.entry.package.name,
      ),
    );
    expect(downloads.dataset.direction).toBe('asc');
  });

  it('shows the catalogue when download statistics fail and finishes loading', async () => {
    mocks.invoke.mockImplementation((command: string) => {
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

  it.each(['downloads', 'published'])(
    'avoids intermediate ordering while fetching counts for the current %s sort',
    async (metric) => {
      const catalogue = structuredClone(snapshot);
      const second = structuredClone(catalogue.widgets[0]);
      second.entry.package.name = '@aidget-community/zebra';
      second.entry.publishedAt = '2026-10-24T00:00:00Z';
      catalogue.widgets.push(second);
      let resolveCounts!: (counts: DownloadCount[]) => void;
      mocks.invoke.mockImplementation((command: string) => {
        if (command === 'community_list') {
          return Promise.resolve(catalogue);
        }
        if (command === 'community_downloads') {
          return new Promise((resolve) => {
            resolveCounts = resolve;
          });
        }
        return Promise.resolve();
      });
      const { root } = page();
      await vi.waitFor(() => expect(resolveCounts).toBeTypeOf('function'));
      expect(root.querySelectorAll('article')).toHaveLength(0);
      expect(root.getAttribute('aria-busy')).toBe('true');
      expect(root.querySelector('.community-status')?.textContent).toBe('Fetching data...');
      if (metric === 'published') {
        root.querySelector<HTMLButtonElement>('[data-sort="published"]')!.click();
        expect(root.querySelectorAll('article')).toHaveLength(2);
        expect(root.getAttribute('aria-busy')).toBe('false');
        expect(root.querySelector<HTMLElement>('[data-download-count]')?.hidden).toBe(true);
      }
      resolveCounts([{ package: second.entry.package.name, weekly: 10, error: null }]);
      await vi.waitFor(() => expect(root.textContent).toContain('10/week'));
      expect(root.querySelector<HTMLElement>('article')?.dataset.package).toBe(
        metric === 'published' ? '@aidget-community/clock' : second.entry.package.name,
      );
      if (metric === 'published') {
        root.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!.click();
      }
      expect(root.querySelector<HTMLElement>('article')?.dataset.package).toBe(
        second.entry.package.name,
      );
    },
  );
});
