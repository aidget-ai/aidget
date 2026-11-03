import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import { translate, type Locale, type MessageKey } from './locale';
import { resolvedLocale } from './preferences';

/** What the backend reports about the update flow, in the order it settles. */
export type UpdateState =
  | 'idle'
  | 'checking'
  | 'latest'
  | 'no_release'
  | 'source'
  | 'found'
  | 'downloading'
  | 'ready'
  | 'installing'
  | 'error';

/** The payload `update_*` commands answer with. Mirrors `update::UpdateSnapshot`. */
export interface UpdateSnapshot {
  state: UpdateState;
  current: string;
  latest: string | null;
  error: string | null;
  done: number | null;
  total: number | null;
  notes: string | null;
  staged: boolean;
}

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
  locale: () => Locale,
  onAutoDownload: (enabled: boolean) => Promise<void>,
  onFault: (message: string) => void,
) {
  const status = root.querySelector<HTMLElement>('[data-update-status]')!;
  const action = root.querySelector<HTMLButtonElement>('[data-update-action]')!;
  const actionLabel = action.querySelector<HTMLElement>('[data-update-label]')!;
  const headerLabel = headerAction.querySelector<HTMLElement>('[data-update-label]')!;
  const autoDownloadSwitch = root.querySelector<HTMLButtonElement>('[data-update-auto-download]')!;
  const notesRow = root.querySelector<HTMLElement>('[data-update-notes-row]')!;
  const notes = root.querySelector<HTMLElement>('[data-update-notes]')!;
  const t = (key: MessageKey) => translate(resolvedLocale(locale()), key);

  let snapshot: UpdateSnapshot | undefined;
  let autoDownload = false;
  let aboutActive = false;
  let notice: UpdateState | undefined;
  let noticeTimer: number | undefined;
  let previousState: UpdateState | undefined;
  let poll: number | undefined;

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
    const icon = button.querySelector<HTMLElement>('[data-update-icon]');
    if (icon) {
      icon.hidden = mode === 'check';
    }
  }

  function render() {
    autoDownloadSwitch.setAttribute('aria-checked', String(autoDownload));
    autoDownloadSwitch.setAttribute('aria-label', t('autoDownload'));

    const current = snapshot;
    if (!current) {
      status.textContent = t('updateIdle');
      renderAction(action, actionLabel, t('checkNow'), 'check', false);
      headerAction.hidden = true;
      notesRow.hidden = true;
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
          : current.state === 'downloading' || (current.state === 'checking' && current.latest)
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

    const announced = current.state === 'found' || current.state === 'ready';
    notesRow.hidden = !announced;
    notes.textContent = announced ? (current.notes ?? '') : '';

    const headerActionable =
      current.state === 'found' ||
      current.state === 'ready' ||
      current.state === 'downloading' ||
      current.state === 'installing' ||
      (current.state === 'checking' && current.latest !== null) ||
      (current.state === 'error' && current.latest !== null);
    headerAction.hidden = aboutActive || !headerActionable;

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
  autoDownloadSwitch.addEventListener('click', () => {
    if (autoDownloadSwitch.disabled) {
      return;
    }
    const next = !autoDownload;
    autoDownloadSwitch.disabled = true;
    void onAutoDownload(next)
      .catch((error: unknown) => onFault(String(error)))
      .finally(() => {
        autoDownloadSwitch.disabled = false;
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
