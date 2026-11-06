import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type { Language, UpdateNotes, UpdateSnapshot, UpdateState } from '../types/rust';
import { translate, type MessageKey } from './locale';
import { renderReleaseNotes } from './release-notes';

export type { ReleaseNote, UpdateNotes, UpdateSnapshot, UpdateState } from '../types/rust';

const stateMessages: Record<UpdateState, MessageKey> = {
  idle: 'updateIdle',
  checking: 'updateChecking',
  latest: 'updateLatest',
  no_release: 'updateNoRelease',
  source: 'updateSource',
  found: 'updateFound',
  downloading: 'updateDownloading',
  ready: 'updateReady',
  installing: 'updateInstalling',
  error: 'updateFailed',
};

const NOTICE_STATES = new Set<UpdateState>(['latest', 'no_release', 'source']);
const NOTICE_DURATION = 3_000;
const POLL_INTERVAL = 400;
const UPDATE_EVENT = 'aidget://update';

/**
 * The update action shared by the About version row and the global page header.
 *
 * The About row is always present so it can start a check. The header mirrors
 * the same action only when there is an update to handle, and stays hidden on
 * About because the row already provides it.
 */
export function createUpdatePage(
  root: HTMLElement,
  headerAction: HTMLButtonElement,
  locale: () => Language,
  onAutoDownload: (enabled: boolean) => Promise<void>,
  onFault: (message: string) => void,
  openExternal: (url: string) => void,
) {
  const status = root.querySelector<HTMLElement>('[data-update-status]')!;
  const action = root.querySelector<HTMLButtonElement>('[data-update-action]')!;
  const actionLabel = action.querySelector<HTMLElement>('[data-update-label]')!;
  const headerLabel = headerAction.querySelector<HTMLElement>('[data-update-label]')!;
  const autoDownloadSwitch = root.querySelector<HTMLButtonElement>('[data-update-auto-download]')!;
  const versionDebugNote = root.querySelector<HTMLElement>('[data-update-version-debug-note]')!;
  const debugNote = root.querySelector<HTMLElement>('[data-update-debug-note]')!;
  const notesOpen = root.querySelector<HTMLButtonElement>('[data-update-notes-open]')!;
  const notesEmpty = root.querySelector<HTMLElement>('[data-update-notes-empty]')!;
  const notesLimitNote = root.querySelector<HTMLElement>('[data-update-notes-limit-note]')!;
  const t = (key: MessageKey) => translate(locale(), key);
  const notesDialog = document.createElement('dialog');
  notesDialog.className = 'community-dialog update-notes-dialog';
  notesDialog.dataset.updateNotesDialog = '';
  notesDialog.tabIndex = -1;
  notesDialog.setAttribute('aria-labelledby', 'update-notes-title');
  root.append(notesDialog);

  let snapshot: UpdateSnapshot | undefined;
  let autoDownload = false;
  let autoDownloadSaving = false;
  let notesLoading = false;
  let notesRefreshRequired = false;
  let debugNotesLoaded = false;
  let aboutActive = false;
  let notice: UpdateState | undefined;
  let noticeTimer: number | undefined;
  let previousState: UpdateState | undefined;
  let poll: number | undefined;
  let renderedNotes: UpdateNotes | undefined;
  let renderedNotesLocale: Language | undefined;

  function stopNotice() {
    window.clearTimeout(noticeTimer);
    notice = undefined;
  }

  function showNotice(state: UpdateState) {
    if (notice === state) {
      return;
    }
    stopNotice();
    notice = state;
    noticeTimer = window.setTimeout(() => {
      notice = undefined;
      render();
    }, NOTICE_DURATION);
  }

  function actionCommand(current: UpdateSnapshot) {
    const actionable =
      current.state === 'found' ||
      current.state === 'ready' ||
      (current.state === 'error' && current.latest !== null);
    return actionable ? 'update_apply' : 'update_check';
  }

  function renderAction(
    button: HTMLButtonElement,
    label: HTMLElement,
    text: string,
    mode: 'check' | 'update' | 'progress',
    busy: boolean,
  ) {
    label.textContent = text;
    button.dataset.updateMode = mode;
    button.disabled = busy;
    button.setAttribute('aria-busy', String(busy));
    const icon = button.querySelector<HTMLElement>('.update-action-icon');
    if (icon) {
      icon.toggleAttribute('hidden', mode === 'check');
    }
  }

  function renderNotesDialog(current: UpdateNotes) {
    const currentLocale = locale();
    const previous = renderedNotes;
    const unchanged =
      previous !== undefined &&
      currentLocale === renderedNotesLocale &&
      current.from === previous.from &&
      current.to === previous.to &&
      current.releases.length === previous.releases.length &&
      current.releases.every(
        (release, index) =>
          release.version === previous.releases[index].version &&
          release.body === previous.releases[index].body,
      );
    if (unchanged) {
      return;
    }
    const title = document.createElement('h2');
    title.id = 'update-notes-title';
    title.textContent = t('updateNotesTitle').replace('{version}', current.to.replace(/^v/, ''));
    const content = document.createElement('div');
    content.className = 'community-dialog-content release-notes-content';
    renderReleaseNotes(content, current, locale(), t('otherChanges'), openExternal);
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'community-dialog-close';
    close.textContent = t('close');
    close.addEventListener('click', () => notesDialog.close());
    const heading = document.createElement('div');
    heading.className = 'community-dialog-header update-notes-header';
    heading.append(title);
    // A single release can still cover an installed-to-target upgrade.
    if (versionValue(current.from) !== versionValue(current.to)) {
      const range = document.createElement('p');
      range.className = 'update-notes-range';
      range.dataset.updateNotesRange = '';
      range.textContent = versionRange(current.from, current.to);
      heading.append(range);
    }
    const footer = document.createElement('div');
    footer.className = 'community-dialog-footer';
    footer.append(close);
    notesDialog.replaceChildren(heading, content, footer);
    renderedNotes = current;
    renderedNotesLocale = currentLocale;
  }

  function openNotes(current: UpdateNotes) {
    renderNotesDialog(current);
    if (!notesDialog.open) {
      notesDialog.showModal();
    }
    notesDialog.focus();
  }

  async function showNotes() {
    const current = snapshot;
    if (!current || notesLoading) {
      return;
    }
    if (
      current.notes?.releases.length &&
      !notesRefreshRequired &&
      (!current.isDebugBuild || debugNotesLoaded)
    ) {
      try {
        openNotes(current.notes);
      } catch (error: unknown) {
        onFault(String(error));
      }
      return;
    }
    if (!current.isDebugBuild) {
      return;
    }

    notesLoading = true;
    render();
    try {
      const next = await invoke<UpdateSnapshot>('update_check');
      snapshot = next;
      watch();
      if (next.state === 'error') {
        notesRefreshRequired = true;
        onFault(next.error ?? t('updateFailed'));
        return;
      }
      if (!next.notes?.releases.length) {
        notesRefreshRequired = true;
        return;
      }
      notesRefreshRequired = false;
      debugNotesLoaded = true;
      openNotes(next.notes);
    } catch (error: unknown) {
      notesRefreshRequired = true;
      onFault(String(error));
    } finally {
      notesLoading = false;
      render();
    }
  }

  function render() {
    const current = snapshot;
    const debugBuild = current?.isDebugBuild ?? false;
    autoDownloadSwitch.setAttribute('aria-checked', String(debugBuild ? false : autoDownload));
    autoDownloadSwitch.setAttribute('aria-label', t('autoDownload'));
    autoDownloadSwitch.disabled = debugBuild || autoDownloadSaving;
    autoDownloadSwitch.setAttribute('aria-busy', String(autoDownloadSaving));
    versionDebugNote.hidden = !debugBuild;
    versionDebugNote.textContent = t('devVersionDescription');
    debugNote.hidden = !debugBuild;
    debugNote.textContent = t('autoDownloadDebugUnavailable');
    notesLimitNote.hidden = !debugBuild;
    notesLimitNote.textContent = t('devUpdateNotesDescription');
    if (debugBuild) {
      autoDownloadSwitch.setAttribute('aria-describedby', debugNote.id);
    } else {
      autoDownloadSwitch.removeAttribute('aria-describedby');
    }

    if (!current) {
      status.textContent = t('updateIdle');
      renderAction(action, actionLabel, t('checkNow'), 'check', false);
      headerAction.hidden = true;
      notesOpen.disabled = true;
      notesEmpty.hidden = true;
      return;
    }

    if (NOTICE_STATES.has(current.state) && previousState !== current.state && !notice) {
      showNotice(current.state);
    } else {
      if (!NOTICE_STATES.has(current.state)) {
        stopNotice();
      }
    }
    previousState = current.state;

    const busy =
      current.state === 'checking' ||
      current.state === 'downloading' ||
      current.state === 'installing';
    const percent =
      current.state === 'downloading' && current.total
        ? `${Math.floor(((current.done ?? 0) / current.total) * 100)}%`
        : '';
    const label =
      notice === current.state
        ? t(stateMessages[current.state])
        : current.state === 'found' ||
            current.state === 'ready' ||
            (current.state === 'error' && current.latest !== null)
          ? t('updateAction')
          : current.state === 'downloading'
            ? [t('updateDownloading'), percent].filter(Boolean).join(' ')
            : current.state === 'installing'
              ? t('updateInstalling')
              : current.state === 'checking'
                ? t('updateChecking')
                : t('checkNow');
    const mode =
      notice === current.state ||
      (!current.latest &&
        current.state !== 'found' &&
        current.state !== 'ready' &&
        current.state !== 'downloading' &&
        current.state !== 'installing')
        ? 'check'
        : busy
          ? 'progress'
          : 'update';
    action.hidden = current.isDebugBuild;
    renderAction(action, actionLabel, label, mode, busy);
    renderAction(headerAction, headerLabel, label, mode, busy);

    const detail =
      current.state === 'found' || current.state === 'ready'
        ? current.latest
        : current.state === 'error'
          ? current.error
          : [current.latest, percent].filter(Boolean).join(' ') || null;
    status.textContent = detail
      ? `${t(stateMessages[current.state])} ${detail}`
      : t(stateMessages[current.state]);

    notesOpen.textContent = t('open');
    const hasNotes = Boolean(current.notes?.releases.length);
    const notesUnavailable =
      current.isDebugBuild &&
      notesRefreshRequired &&
      !notesLoading &&
      !hasNotes &&
      current.state !== 'error';
    notesOpen.disabled = notesLoading || (!hasNotes && (!current.isDebugBuild || notesUnavailable));
    notesOpen.textContent = notesLoading ? t('loading') : t('open');
    notesOpen.setAttribute('aria-busy', String(notesLoading));
    notesEmpty.hidden = !notesUnavailable;
    notesEmpty.textContent = t('noUpdateNotes');
    if (notesDialog.open && current.notes) {
      try {
        renderNotesDialog(current.notes);
      } catch (error: unknown) {
        onFault(String(error));
      }
    }

    const headerActionable =
      current.state === 'found' ||
      current.state === 'ready' ||
      current.state === 'downloading' ||
      current.state === 'installing' ||
      (current.state === 'checking' && current.latest !== null) ||
      (current.state === 'error' && current.latest !== null);
    headerAction.hidden = current.isDebugBuild || aboutActive || !headerActionable;

    // Keep the verified download when this recheck fails; surface the error
    // separately so the user can retry.
    if (current.state === 'ready' && current.error) {
      onFault(current.error);
    }
  }

  function watch() {
    window.clearTimeout(poll);
    if (snapshot?.state !== 'checking' && snapshot?.state !== 'downloading') {
      return;
    }
    poll = window.setTimeout(() => {
      void invoke<UpdateSnapshot>('update_state')
        .then((next) => {
          snapshot = next;
          render();
          watch();
        })
        .catch((error: unknown) => onFault(String(error)));
    }, POLL_INTERVAL);
  }

  async function run(command: string) {
    try {
      snapshot = await invoke<UpdateSnapshot>(command);
      render();
      watch();
    } catch (error: unknown) {
      onFault(String(error));
    }
  }

  function runAction() {
    if (!snapshot) {
      void run('update_check');
      return;
    }
    void run(actionCommand(snapshot));
  }

  // The launch check runs without anyone asking, so the window cannot discover it by
  // polling: it is told. `refresh` still runs first, for a snapshot that settled
  // before this module was listening.
  void listen<UpdateSnapshot>(UPDATE_EVENT, (event) => {
    snapshot = event.payload;
    render();
    watch();
  });

  action.addEventListener('click', runAction);
  headerAction.addEventListener('click', runAction);
  notesOpen.addEventListener('click', showNotes);
  autoDownloadSwitch.addEventListener('click', () => {
    if (autoDownloadSwitch.disabled) {
      return;
    }
    const next = !autoDownload;
    autoDownloadSaving = true;
    render();
    void onAutoDownload(next)
      .catch((error: unknown) => onFault(String(error)))
      .finally(() => {
        autoDownloadSaving = false;
        render();
      });
  });

  return {
    /** Redraw from the last snapshot, after a preference change for instance. */
    render,
    /** Suppress the duplicate header action while About is visible. */
    setAboutActive(active: boolean) {
      aboutActive = active;
      render();
    },
    /** Tell the action whether automatic downloading is on. */
    setAutoDownload(enabled: boolean) {
      autoDownload = enabled;
      render();
    },
    /** Read the snapshot the backend already holds, without asking the feed. */
    async refresh() {
      try {
        snapshot = await invoke<UpdateSnapshot>('update_state');
        render();
        watch();
      } catch (error: unknown) {
        onFault(String(error));
      }
    },
  };
}

function versionValue(version: string) {
  return `v${version.replace(/^v/, '')}`;
}

function versionRange(from: string, to: string) {
  const start = versionValue(from);
  const end = versionValue(to);
  return start === end ? start : `${start} → ${end}`;
}
