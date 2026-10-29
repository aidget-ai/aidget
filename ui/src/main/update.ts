import { invoke } from '@tauri-apps/api/core';

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

/** How often a running step is re-read, so progress and the settle appear. */
const POLL_INTERVAL = 400;

/**
 * The update row of the About page.
 *
 * The window decides what to offer from what is on disk, not from what the last
 * network step did: a failed check, download or install must not take away work
 * that is still possible.
 */
export function createUpdatePage(
  root: HTMLElement,
  locale: () => Locale,
  onAutoDownload: (enabled: boolean) => Promise<void>,
  onFault: (message: string) => void,
) {
  const status = root.querySelector<HTMLElement>('[data-update-status]')!;
  const checkButton = root.querySelector<HTMLButtonElement>('[data-update-check]')!;
  const downloadButton = root.querySelector<HTMLButtonElement>('[data-update-download]')!;
  const installButton = root.querySelector<HTMLButtonElement>('[data-update-install]')!;
  const autoDownloadSwitch = root.querySelector<HTMLButtonElement>('[data-update-auto-download]')!;
  const notesRow = root.querySelector<HTMLElement>('[data-update-notes-row]')!;
  const notes = root.querySelector<HTMLElement>('[data-update-notes]')!;
  const t = (key: MessageKey) => translate(resolvedLocale(locale()), key);

  let snapshot: UpdateSnapshot | undefined;
  let autoDownload = false;
  let poll: number | undefined;

  function render() {
    // The three buttons carry `data-message`, so the locale pass names them; only
    // the switch needs its accessible name here.
    autoDownloadSwitch.setAttribute('aria-checked', String(autoDownload));
    autoDownloadSwitch.setAttribute('aria-label', t('autoDownload'));

    const current = snapshot;
    const staged = current?.staged ?? false;
    const busy =
      current?.state === 'checking' ||
      current?.state === 'downloading' ||
      current?.state === 'installing';
    const downloadable =
      !busy &&
      !staged &&
      Boolean(current?.latest) &&
      current?.state !== 'idle' &&
      current?.state !== 'latest' &&
      current?.state !== 'no_release' &&
      current?.state !== 'source';
    downloadButton.hidden = !downloadable;
    installButton.hidden = !staged || busy;

    if (!current) {
      status.textContent = '—';
      notesRow.hidden = true;
      return;
    }
    const label = t(stateMessages[current.state]);
    const percent =
      current.state === 'downloading' && current.total
        ? `${Math.floor(((current.done ?? 0) / current.total) * 100)}%`
        : undefined;
    const detail =
      current.state === 'found' || current.state === 'ready'
        ? current.latest
        : current.state === 'error'
          ? current.error
          : percent;
    status.textContent = detail ? `${label} ${detail}` : label;

    // The notes belong to the update: an up-to-date build has nothing to say.
    const announced = current.state === 'found' || current.state === 'ready';
    const shown = announced ? current.notes : null;
    notesRow.hidden = !shown;
    notes.textContent = shown ?? '';

    // A failed check next to an update that is already downloaded: keep the
    // update and say why the check failed, instead of replacing one with the
    // other.
    if (current.state === 'ready' && current.error) {
      onFault(current.error);
    }
  }

  /** Re-read the snapshot while a step runs, then stop. */
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

  checkButton.addEventListener('click', () => void run('update_check'));
  downloadButton.addEventListener('click', () => void run('update_download'));
  installButton.addEventListener('click', () => void run('update_install'));
  autoDownloadSwitch.addEventListener('click', () => {
    const next = !autoDownload;
    autoDownloadSwitch.disabled = true;
    void onAutoDownload(next)
      .then(() => {
        autoDownload = next;
      })
      .finally(() => {
        autoDownloadSwitch.disabled = false;
        render();
      });
  });

  return {
    /** Redraw from the last snapshot, after a preference change for instance. */
    render,
    /** Tell the row whether automatic downloading is on. */
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
