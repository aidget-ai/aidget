import { Channel, invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { createElement, ExternalLink, RefreshCw, X } from 'lucide';

import type {
  CachedCommunitySnapshot,
  CommunityEntry,
  CommunitySnapshot,
  DownloadCount,
  PackageAuthor,
} from '../types/rust';
import { createCommunityCard } from './community-card';
import { createCommunityMedia } from './community-media';
import { createCommunitySort } from './community-sort';
import { translate, type Locale, type MessageKey } from './locale';
import { resolvedLocale } from './preferences';
import { renderWidgetDetailsContent } from './widget-details';

/** Format npm's count without confusing an absent response with a real zero. */
export function weeklyDownloads(value: number | null, locale: string) {
  if (value === null) {
    return null;
  }
  const [divisor, suffix] =
    value >= 1e9 ? [1e9, 'b'] : value >= 1e6 ? [1e6, 'm'] : value >= 1e3 ? [1e3, 'k'] : [1, ''];
  return (
    new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(value / divisor) + suffix
  );
}
function authorName(author: PackageAuthor | null) {
  return typeof author === 'string' ? author : (author?.name ?? '');
}
/** Render a community catalogue. Remote HTML never enters this document. */
export function createCommunityPage(
  root: HTMLElement,
  locale: () => Locale,
  installed: () => Promise<void>,
  hasGithubToken: () => boolean,
  notify: (message: string) => void,
) {
  let snapshot: CommunitySnapshot | undefined;
  let loading = false;
  let countsPending = false;
  let countsGeneration = 0;
  let requested = false;
  let visible = false;
  let checkedAt = 0;
  let lastAttempt = -Infinity;
  let forceCheck = false;
  let loadError = '';
  let detailLocale: Locale | undefined;
  const cards = new Map<string, ReturnType<typeof createCommunityCard>>();
  const media = createCommunityMedia(locale);
  const detailMedia: ReturnType<typeof media.create>[] = [];
  let generation = 0;
  let installing: string | undefined;
  let selected: { entry: CommunityEntry; commit: string } | undefined;
  let installError = '';
  let linkError = '';
  const counts = new Map<string, DownloadCount>();
  const t = (key: MessageKey) => translate(resolvedLocale(locale()), key);
  const header = document.createElement('div');
  header.className = 'community-heading';
  const heading = document.createElement('h1');
  heading.id = 'community-widgets-title';
  heading.className = 'visually-hidden';
  const refresh = document.createElement('button');
  refresh.type = 'button';
  refresh.className = 'community-refresh';
  refresh.append(createElement(RefreshCw, { width: 15, height: 15, 'aria-hidden': 'true' }));
  const status = document.createElement('p');
  status.className = 'community-status';
  status.setAttribute('role', 'status');
  const banner = document.createElement('p');
  banner.className = 'community-banner';
  banner.hidden = localStorage.getItem('aidget.communityBannerDismissed') === 'true';
  const introduction = document.createTextNode('');
  const invitation = document.createTextNode('');
  const repository = document.createElement('a');
  repository.className = 'community-repository';
  repository.href = 'https://github.com/aidget-ai/widgets';
  const repositoryLabel = document.createElement('span');
  repository.append(
    repositoryLabel,
    createElement(ExternalLink, { width: 13, height: 13, 'aria-hidden': 'true' }),
  );
  const dismissBanner = document.createElement('button');
  dismissBanner.type = 'button';
  dismissBanner.className = 'community-banner-close';
  dismissBanner.append(createElement(X, { width: 14, height: 14, 'aria-hidden': 'true' }));
  dismissBanner.addEventListener('click', () => {
    localStorage.setItem('aidget.communityBannerDismissed', 'true');
    banner.hidden = true;
  });
  banner.append(introduction, repository, invitation, dismissBanner);
  repository.addEventListener('click', (event) => {
    event.preventDefault();
    linkError = '';
    void openUrl(repository.href).catch((error: unknown) => {
      linkError = `${t('openLinkFailed')}: ${String(error)}`;
      render();
    });
  });
  const toolbar = document.createElement('div');
  toolbar.className = 'community-toolbar';
  const sort = createCommunitySort(locale, render);
  toolbar.append(sort.element, refresh);
  const grid = document.createElement('div');
  grid.className = 'widget-grid community-grid';
  const dialog = document.createElement('dialog');
  dialog.className = 'community-dialog';
  dialog.tabIndex = -1;
  dialog.setAttribute('aria-labelledby', 'community-detail-title');
  header.append(heading);
  root.replaceChildren(banner, header, toolbar, status, grid, dialog);
  refresh.addEventListener('click', () => void load());
  dialog.addEventListener('close', () => {
    selected = undefined;
    for (const view of detailMedia.splice(0)) {
      view.dispose();
    }
  });

  function actionLabel(entry: CommunityEntry) {
    if (installing === entry.entry.package.name) {
      return t('communityInstalling');
    }
    if (entry.installed) {
      return t('communityInstalled');
    }
    if (entry.idConflict) {
      return t('communityIdConflict');
    }
    if (!entry.supported) {
      return t('communityUnsupported');
    }
    return t('communityInstall');
  }
  function installAction(entry: CommunityEntry, commit: string) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'community-install secondary-button';
    button.textContent = actionLabel(entry);
    button.disabled = !!installing || entry.installed || entry.idConflict || !entry.supported;
    button.addEventListener('click', () => {
      void install(entry, commit);
    });
    return button;
  }
  function renderCount(element: HTMLElement, name: string) {
    const count = counts.get(name);
    const number = weeklyDownloads(count?.weekly ?? null, resolvedLocale(locale()));
    element.hidden = number === null;
    element.textContent = number === null ? '' : `↓ ${number}${t('communityPerWeek')}`;
    element.title = `${t('communityNpmDownloads')}${count?.weekly === null ? '' : `: ${count?.weekly ?? ''}`}`;
  }
  function render() {
    const waitingForOrder = countsPending && sort.value.startsWith('downloads');
    const busy = loading || waitingForOrder;
    heading.textContent = t('communityWidgetsTitle');
    refresh.title = t(busy ? 'communityRefreshing' : 'communityRefresh');
    refresh.setAttribute('aria-label', refresh.title);
    refresh.setAttribute('aria-busy', String(busy));
    refresh.disabled = busy || !!installing;
    refresh.hidden = busy && !grid.childElementCount;
    introduction.textContent = t('communityIntroduction');
    repositoryLabel.textContent = t('communityRepository');
    invitation.textContent = t('communityInvitation');
    dismissBanner.title = t('communityDismissBanner');
    dismissBanner.setAttribute('aria-label', dismissBanner.title);
    sort.render(!snapshot || !!installing, countsPending);
    root.setAttribute('aria-busy', String(busy));
    const cached =
      snapshot && loadError
        ? `${t('communityCached')} · ${t('communityLastChecked')}: ${new Date(checkedAt).toLocaleString(resolvedLocale(locale()))}\n`
        : '';
    status.textContent =
      linkError ||
      (installError && !dialog.open ? installError : '') ||
      (loadError
        ? cached + loadError
        : busy && !grid.childElementCount
          ? t('communityFetching')
          : snapshot && !snapshot.widgets.length
            ? t('communityEmpty')
            : '');
    renderGrid();
    media.render();
    if (selected && dialog.open) {
      if (detailLocale !== locale()) {
        renderDetails(selected.entry, selected.commit);
      }
      const button = dialog.querySelector<HTMLButtonElement>('.community-install')!;
      button.textContent = actionLabel(selected.entry);
      button.disabled =
        !!installing ||
        selected.entry.installed ||
        selected.entry.idConflict ||
        !selected.entry.supported;
      dialog.querySelector('.community-install-error')!.textContent = installError;
    }
  }
  function renderGrid() {
    // Keep the previous list until the selected download order can be determined.
    if (countsPending && sort.value.startsWith('downloads')) {
      return;
    }
    const widgets = snapshot?.widgets.slice() ?? [];
    widgets.sort((a, b) => {
      const left = sort.value.startsWith('published')
        ? Date.parse(a.entry.publishedAt)
        : counts.get(a.entry.package.name)?.weekly;
      const right = sort.value.startsWith('published')
        ? Date.parse(b.entry.publishedAt)
        : counts.get(b.entry.package.name)?.weekly;
      // Missing telemetry always follows real counts, including a real zero.
      if (left == null || right == null) {
        if (left != null) {
          return -1;
        }
        if (right != null) {
          return 1;
        }
      } else if (left !== right) {
        return (left - right) * (sort.value.endsWith('asc') ? 1 : -1);
      }
      return a.entry.package.name.localeCompare(b.entry.package.name, 'en');
    });
    const names = new Set(widgets.map((entry) => entry.entry.package.name));
    for (const [name, view] of cards) {
      if (!names.has(name)) {
        view.dispose();
        cards.delete(name);
      }
    }
    for (const [index, entry] of widgets.entries()) {
      const name = entry.entry.package.name;
      let view = cards.get(name);
      if (!view) {
        view = createCommunityCard(entry, snapshot!.commit, media, root, locale, showDetails);
        cards.set(name, view);
      }
      view.update(entry, snapshot!.commit, !!installing, actionLabel(entry));
      renderCount(view.count, name);
      if (grid.children[index] !== view.element) {
        const focus = document.activeElement;
        grid.insertBefore(view.element, grid.children[index] ?? null);
        if (
          focus instanceof HTMLElement &&
          view.element.contains(focus) &&
          document.activeElement !== focus
        ) {
          focus.focus({ preventScroll: true });
        }
      }
    }
  }
  function renderDetails(entry: CommunityEntry, commit: string) {
    const { config, package: pkg } = entry.entry;
    const title = document.createElement('h2');
    title.id = 'community-detail-title';
    title.textContent = config.title;
    const identity = document.createElement('p');
    identity.className = 'community-detail-identity';
    identity.textContent = `${authorName(pkg.author)} · ${pkg.name} · v${pkg.version}`;
    detailLocale = locale();
    const previousMedia = detailMedia.splice(0);
    const content = renderWidgetDetailsContent(config, pkg.description, [], locale());
    for (const [index, path] of config.screenshots.entries()) {
      let view = previousMedia[index];
      if (view) {
        view.setRoot(content);
      } else {
        view = media.create(entry, path, commit, content, true);
      }
      detailMedia.push(view);
      content.querySelector('.community-detail-images')!.append(view.element);
    }
    for (const view of previousMedia.slice(config.screenshots.length)) {
      view.dispose();
    }
    content.prepend(identity);
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'community-dialog-close';
    close.textContent = t('communityClose');
    close.addEventListener('click', () => dialog.close());
    const error = document.createElement('p');
    error.textContent = installError;
    error.className = 'community-install-error';
    error.setAttribute('role', 'alert');
    const header = document.createElement('div');
    header.className = 'community-dialog-header';
    header.append(title, installAction(entry, commit));
    const footer = document.createElement('div');
    footer.className = 'community-dialog-footer';
    footer.append(error, close);
    dialog.replaceChildren(header, content, footer);
  }
  function showDetails(entry: CommunityEntry, commit: string) {
    installError = '';
    selected = { entry, commit };
    renderDetails(entry, selected.commit);
    dialog.showModal();
    media.retryFailed(entry, selected.commit);
    dialog.focus();
  }
  async function install(entry: CommunityEntry, commit: string) {
    if (!snapshot || installing) {
      return;
    }
    installing = entry.entry.package.name;
    installError = '';
    render();
    try {
      await invoke('community_install', { commit, package: entry.entry.package.name });
      entry.installed = true;
      const current = snapshot?.widgets.find(
        (value) => value.entry.package.name === entry.entry.package.name,
      );
      if (current) {
        current.installed = true;
      }
      await installed();
      dialog.close();
      notify(t('communityInstalledNotice'));
    } catch (error) {
      installError = `${t('communityInstallFailed')}: ${String(error)}`;
    } finally {
      installing = undefined;
      render();
    }
  }
  async function load() {
    if (loading || installing) {
      return;
    }
    const current = ++generation;
    requested = true;
    loading = true;
    linkError = '';
    loadError = '';
    lastAttempt = Date.now();
    forceCheck = false;
    render();
    try {
      const next = await invoke<CommunitySnapshot>('community_list');
      if (current !== generation) {
        return;
      }
      snapshot = next;
      checkedAt = Date.now();
      fetchCounts(next);
    } catch (error) {
      if (current !== generation) {
        return;
      }
      loadError = `${t('communityLoadFailed')}: ${String(error)}${
        hasGithubToken() ? '' : `\n${t('communityGithubTokenHint')}`
      }`;
    } finally {
      if (current === generation) {
        loading = false;
        render();
      }
    }
  }
  function fetchCounts(next: CommunitySnapshot) {
    const current = ++countsGeneration;
    countsPending = true;
    const onCount = new Channel<DownloadCount>((value) => {
      if (current !== countsGeneration || !countsPending) {
        return;
      }
      counts.set(value.package, value);
      // Update only telemetry so incoming counts preserve card order and focus.
      for (const element of grid.querySelectorAll<HTMLElement>('[data-download-count]')) {
        if (element.dataset.downloadCount === value.package) {
          renderCount(element, value.package);
        }
      }
    });
    void invoke<DownloadCount[]>('community_downloads', {
      packages: next.widgets.map((e) => e.entry.package.name),
      onCount,
    })
      .then((values) => {
        if (current !== countsGeneration) {
          return;
        }
        countsPending = false;
        counts.clear();
        for (const value of values) {
          counts.set(value.package, value);
        }
        render();
      })
      .catch(() => {
        if (current !== countsGeneration) {
          return;
        }
        countsPending = false;
        // Statistics are optional; a failed refresh must hide any stale counts.
        counts.clear();
        render();
      });
  }
  async function restore() {
    const current = ++generation;
    requested = true;
    loading = true;
    render();
    try {
      const cached = await invoke<CachedCommunitySnapshot | null>('community_cached');
      if (current !== generation) {
        return;
      }
      if (cached) {
        snapshot = cached.snapshot;
        checkedAt = cached.checkedAt * 1000;
        lastAttempt = Math.max(lastAttempt, checkedAt);
        fetchCounts(snapshot);
      }
    } catch (error) {
      if (current === generation) {
        loadError = `${t('communityLoadFailed')}: ${String(error)}`;
      }
      return;
    } finally {
      if (current === generation) {
        loading = false;
        render();
      }
    }
    if (current === generation && (forceCheck || Date.now() - lastAttempt >= 5 * 60_000)) {
      void load();
    }
  }
  return {
    /** Restore cached content and check main only when entering or explicitly invalidated. */
    show() {
      const entering = !visible;
      visible = true;
      media.setActive(true);
      if (!requested) {
        void restore();
      } else if (entering && Date.now() - lastAttempt >= 5 * 60_000) {
        void load();
      } else {
        render();
      }
    },
    /** Cancel queued work when leaving the community panel. */
    hide() {
      visible = false;
      media.setActive(false);
      if (dialog.open) {
        dialog.close();
      }
    },
    /** Recompute local availability; credentials additionally invalidate the check interval. */
    invalidate(credentialsChanged = false) {
      requested = false;
      if (credentialsChanged) {
        forceCheck = true;
      }
      if (visible && !installing) {
        void restore();
      }
    },
    /** Update translated labels without an additional network request. */
    render,
  };
}
