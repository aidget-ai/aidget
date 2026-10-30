import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { createElement, ExternalLink, LayoutGrid, RefreshCw, ShieldCheck, X } from 'lucide';

import type {
  CommunityEntry,
  CommunitySnapshot,
  DownloadCount,
  PackageAuthor,
} from '../types/rust';
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
) {
  let snapshot: CommunitySnapshot | undefined;
  let loading = false;
  let countsPending = false;
  let requested = false;
  let generation = 0;
  let installing: string | undefined;
  let selected: { entry: CommunityEntry; commit: string } | undefined;
  let installError = '';
  let linkError = '';
  const counts = new Map<string, DownloadCount>();
  const media = new Map<string, Promise<string>>();
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
  });

  function mediaUrl(entry: CommunityEntry, path: string, commit = snapshot!.commit) {
    const key = `${commit}/${entry.entry.package.name}/${path}`;
    let pending = media.get(key);
    if (!pending) {
      pending = invoke<string>('community_media', {
        commit,
        package: entry.entry.package.name,
        path,
      });
      media.set(key, pending);
    }
    return pending;
  }
  function logo(entry: CommunityEntry) {
    const box = document.createElement('span');
    box.className = 'community-logo';
    box.setAttribute('aria-hidden', 'true');
    const placeholder = document.createElement('span');
    placeholder.className = 'community-logo-default';
    placeholder.append(createElement(LayoutGrid, { width: 24, height: 24, 'stroke-width': 1.6 }));
    box.append(placeholder);
    if (entry.entry.config.logo) {
      const image = document.createElement('img');
      image.alt = '';
      image.width = 32;
      image.height = 32;
      void mediaUrl(entry, entry.entry.config.logo)
        .then((url) => {
          image.src = url;
          box.replaceChildren(image);
        })
        .catch((error) => {
          box.title = `${t('communityLogoFailed')}: ${String(error)}`;
        });
    }
    return box;
  }
  function screenshot(entry: CommunityEntry, path: string, commit = snapshot!.commit) {
    const box = document.createElement('div');
    box.className = 'preview-stage community-preview';
    const image = document.createElement('img');
    image.alt = entry.entry.config.title;
    image.className = 'preview-image';
    image.width = 720;
    image.height = 440;
    image.decoding = 'async';
    const message = document.createElement('span');
    message.textContent = t('loading');
    box.append(message);
    void mediaUrl(entry, path, commit)
      .then((url) => {
        image.onload = () => box.replaceChildren(image);
        image.onerror = () => {
          message.textContent = t('communityScreenshotFailed');
        };
        image.src = url;
      })
      .catch((error) => {
        message.textContent = t('communityScreenshotFailed');
        message.title = String(error);
      });
    return box;
  }
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
  function installAction(entry: CommunityEntry, confirm: boolean, commit = snapshot!.commit) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'community-install secondary-button';
    button.textContent = actionLabel(entry);
    button.disabled =
      loading || !!installing || entry.installed || entry.idConflict || !entry.supported;
    button.addEventListener('click', () => {
      if (confirm) {
        void install(entry, commit);
      } else {
        showDetails(entry);
      }
    });
    return button;
  }
  function renderCard(entry: CommunityEntry) {
    const { config, package: pkg } = entry.entry;
    const card = document.createElement('article');
    card.className = 'widget-card community-card';
    card.dataset.package = pkg.name;
    const top = document.createElement('div');
    top.className = 'widget-info community-card-heading';
    const identity = document.createElement('div');
    identity.className = 'community-identity';
    const name = document.createElement('button');
    name.type = 'button';
    name.className = 'widget-title community-name';
    name.textContent = config.title;
    name.disabled = loading || !!installing;
    name.title = config.title;
    name.addEventListener('click', () => showDetails(entry));
    const by = document.createElement('span');
    by.className = 'community-verified';
    const verifiedLabel = document.createElement('span');
    verifiedLabel.textContent = t('communityVerified');
    by.append(
      createElement(ShieldCheck, {
        width: 13,
        height: 13,
        'stroke-width': 1.7,
        'aria-hidden': 'true',
      }),
      verifiedLabel,
    );
    by.title = authorName(pkg.author);
    identity.append(name, by);
    top.append(logo(entry), identity, installAction(entry, false));
    const preview = document.createElement('button');
    preview.type = 'button';
    preview.disabled = loading || !!installing;
    preview.className = 'community-preview-button';
    preview.setAttribute('aria-label', `${t('communityDetails')}: ${config.title}`);
    preview.addEventListener('click', () => showDetails(entry));
    preview.append(screenshot(entry, config.screenshots[0]));
    const footer = document.createElement('div');
    footer.className = 'community-card-footer';
    const count = document.createElement('span');
    count.dataset.downloadCount = pkg.name;
    renderCount(count, pkg.name);
    const version = document.createElement('span');
    version.textContent = `v${pkg.version}`;
    footer.append(count, version);
    card.append(top, preview, footer);
    return card;
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
    sort.render(!snapshot || loading || !!installing);
    root.setAttribute('aria-busy', String(busy));
    if (busy) {
      status.textContent = grid.childElementCount ? '' : t('communityFetching');
    } else if (snapshot) {
      status.textContent =
        linkError || (installError && !dialog.open)
          ? linkError || installError
          : snapshot.widgets.length
            ? ''
            : t('communityEmpty');
    }
    renderGrid();
    if (selected && dialog.open) {
      renderDetails(selected.entry, selected.commit);
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
    grid.replaceChildren(...widgets.map(renderCard));
  }
  function renderDetails(entry: CommunityEntry, commit: string) {
    const { config, package: pkg } = entry.entry;
    const title = document.createElement('h2');
    title.id = 'community-detail-title';
    title.textContent = config.title;
    const identity = document.createElement('p');
    identity.className = 'community-detail-identity';
    identity.textContent = `${authorName(pkg.author)} · ${pkg.name} · v${pkg.version}`;
    const content = renderWidgetDetailsContent(
      config,
      pkg.description,
      config.screenshots.map((path) => screenshot(entry, path, commit)),
      locale(),
    );
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
    header.append(title, installAction(entry, true, commit));
    const footer = document.createElement('div');
    footer.className = 'community-dialog-footer';
    footer.append(error, close);
    dialog.replaceChildren(header, content, footer);
  }
  function showDetails(entry: CommunityEntry) {
    installError = '';
    selected = { entry, commit: snapshot!.commit };
    renderDetails(entry, selected.commit);
    dialog.showModal();
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
      await installed();
      dialog.close();
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
    countsPending = false;
    linkError = '';
    media.clear();
    render();
    try {
      const next = await invoke<CommunitySnapshot>('community_list');
      if (current !== generation) {
        return;
      }
      snapshot = next;
      countsPending = true;
      void invoke<DownloadCount[]>('community_downloads', {
        packages: next.widgets.map((e) => e.entry.package.name),
      })
        .then((values) => {
          if (current !== generation) {
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
          if (current !== generation) {
            return;
          }
          countsPending = false;
          // Statistics are optional; a failed refresh must hide any stale counts.
          counts.clear();
          render();
        });
    } catch (error) {
      snapshot = undefined;
      status.textContent = `${t('communityLoadFailed')}: ${String(error)}${
        hasGithubToken() ? '' : `\n${t('communityGithubTokenHint')}`
      }`;
    } finally {
      loading = false;
      render();
    }
  }
  return {
    /** Fetch on first visit; explicit refresh retrieves a new main snapshot. */
    show() {
      if (!requested) {
        void load();
      } else {
        render();
      }
    },
    /** Local changes invalidate installed state and are reconciled on next visit. */
    invalidate() {
      requested = false;
    },
    /** Update translated labels without an additional network request. */
    render,
  };
}
