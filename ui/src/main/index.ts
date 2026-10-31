import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  Archive,
  createElement,
  createIcons,
  Info,
  LayoutGrid,
  Minus,
  RotateCcw,
  ServerCog,
  Settings,
  Square,
  Trash,
  UsersRound,
  X,
} from 'lucide';

import brandIcon from '../../../icons/icon.png';
import { renderAgentStrip } from './agent-controls';
import { createCommunityPage } from './community';
import {
  mcpCommandLine,
  type AgentProblem,
  type IntegrationInfo,
  type IntegrationMutation,
  type IntegrationResource,
} from './integration';
import { formatAgentConfigurationFailure, translate, type Locale, type MessageKey } from './locale';
import {
  filteredWidgets,
  isEmpty,
  previewKey,
  previewNeedsRefresh,
  sortedActiveWidgets,
  sortedDeletedWidgets,
  type DeletedWidgetSummary,
  type GithubTokenSummary,
  type MainPage,
  type MainState,
  type TrayClickAction,
  type WidgetFilter,
  type WidgetSortOrder,
  type WidgetSummary,
} from './model';
import { applyTheme, resolvedLocale, type Theme } from './preferences';
import { createPreviewRefresh } from './preview-refresh';
import { createSegmentedControl } from './segmented-control';
import { createSortIcon } from './sort-icon';
import { createMainStartup } from './startup';
import { renderMiddleEllipsis } from './text-ellipsis';
import { createLocalWidgetDetails } from './widget-details';

interface WidgetView {
  root: HTMLElement;
  stage: HTMLButtonElement;
  image: HTMLImageElement;
  placeholder: HTMLElement;
  title: HTMLElement;
  size: HTMLElement;
  toggle: HTMLButtonElement;
  archiveButton: HTMLButtonElement;
  widget: WidgetSummary;
  previewRequest: number;
}

interface DeletedWidgetView {
  root: HTMLElement;
  image: HTMLImageElement;
  placeholder: HTMLElement;
  title: HTMLElement;
  size: HTMLElement;
  restoreButton: HTMLButtonElement;
  deleteButton: HTMLButtonElement;
  trashName: string;
}

type PreferencesSnapshot = Pick<MainState, 'theme' | 'language' | 'trayClickAction' | 'lastPage'>;

const GITHUB_REPOSITORY_URL = 'https://github.com/aidget-ai/aidget';
const grid = document.querySelector<HTMLElement>('[data-grid]')!;
const deletedGrid = document.querySelector<HTMLElement>('[data-deleted-grid]')!;
const empty = document.querySelector<HTMLElement>('[data-empty]')!;
const emptyTitle = document.querySelector<HTMLElement>('[data-empty-title]')!;
const emptyDescription = document.querySelector<HTMLElement>('[data-empty-description]')!;
const fault = document.querySelector<HTMLElement>('[data-fault]')!;
let faultSource: 'operation' | 'broken' | undefined;
const currentGroup = document.querySelector<HTMLElement>('[data-current-group]')!;
const currentPage = document.querySelector<HTMLElement>('[data-current-page]')!;
const filterOptions = document.querySelector<HTMLElement>('[data-filter-options]')!;
const filterControlRoot = document.querySelector<HTMLElement>('[data-widget-filter-control]')!;
const themeControlRoot = document.querySelector<HTMLElement>('[data-theme-options]')!;
const trayClickControlRoot = document.querySelector<HTMLElement>('[data-tray-click-options]')!;
const trayLinuxNote = document.querySelector<HTMLElement>('[data-tray-linux-note]')!;
const recycleFilter = document.querySelector<HTMLButtonElement>('[data-recycle-filter]')!;
const widgetSortButton = document.querySelector<HTMLButtonElement>('[data-widget-sort]')!;
const widgetSortLabel = widgetSortButton.querySelector<HTMLElement>('[data-sort-label]')!;
const brandMark = document.querySelector<HTMLImageElement>('[data-brand-icon]')!;
const version = document.querySelector<HTMLElement>('[data-version]')!;
const githubButton = document.querySelector<HTMLButtonElement>('[data-github]')!;
const autoStart = document.querySelector<HTMLButtonElement>('[data-autostart]')!;
const language = document.querySelector<HTMLSelectElement>('[data-language]')!;
const githubTokenControl = document.querySelector<HTMLElement>('[data-github-token-control]')!;
const githubTokenForm = document.querySelector<HTMLFormElement>('[data-github-token-form]')!;
const githubTokenInput = document.querySelector<HTMLInputElement>('[data-github-token-input]')!;
const githubTokenSave = document.querySelector<HTMLButtonElement>('[data-github-token-save]')!;
const githubTokenConfigured = document.querySelector<HTMLElement>(
  '[data-github-token-configured]',
)!;
const githubTokenMasked = document.querySelector<HTMLElement>('[data-github-token-masked]')!;
const githubTokenRemove = document.querySelector<HTMLButtonElement>('[data-github-token-remove]')!;
const githubTokenError = document.querySelector<HTMLElement>('[data-github-token-error]')!;
const mcpCommandOutput = document.querySelector<HTMLElement>('[data-mcp-command]')!;
const skillDirectoryOutput = document.querySelector<HTMLElement>('[data-skill-directory]')!;
const pageDescription = document.querySelector<HTMLElement>('[data-page-description]')!;
const agentStrips = {
  mcp: document.querySelector<HTMLElement>('[data-agent-strip="mcp"]')!,
  skills: document.querySelector<HTMLElement>('[data-agent-strip="skills"]')!,
};
const integrationPage = document.querySelector<HTMLElement>('[data-view="integrations"]')!;
const integrationStatuses = document.querySelectorAll<HTMLElement>('.integration-status');
const views = new Map<string, WidgetView>();
const deletedViews = new Map<string, DeletedWidgetView>();
const windowApi = getCurrentWindow();
const platform = /Macintosh|Mac OS X/.test(navigator.userAgent)
  ? 'macos'
  : /Windows/.test(navigator.userAgent)
    ? 'windows'
    : 'linux';
document.body.dataset.platform = platform;
const pages: Record<MainPage, { group: MessageKey; title: MessageKey }> = {
  'local-widgets': { group: 'widgetsGroup', title: 'localWidgets' },
  'community-widgets': { group: 'widgetsGroup', title: 'communityWidgets' },
  integrations: { group: 'integrations', title: 'mcpAndSkills' },
  general: { group: 'settings', title: 'general' },
  about: { group: 'settings', title: 'about' },
};

let state: MainState | undefined;
let page: MainPage = 'integrations';
let filter: WidgetFilter = 'all';
let theme: Theme = 'system';
let locale: Locale = 'system';
let trayClickAction: TrayClickAction = platform === 'linux' ? 'menu' : 'window';
let githubTokenOperation: 'save' | 'remove' | undefined;
let githubTokenFailure = '';
let preferencesBusy = false;
let preferenceWrites: Promise<void> = Promise.resolve();
let widgetSort: WidgetSortOrder = 'newest';
let integrationInfo: IntegrationInfo | undefined;
let integrationRequested = false;
let integrationFailed = false;
let integrationBusy = false;
let integrationProblems: AgentProblem[] = [];
let pendingPurge: DeletedWidgetView | undefined;
let purgeTimer: number | undefined;
let previewRefreshActive = false;
let widgetsReadyForPreviews = false;

const community = createCommunityPage(
  document.querySelector<HTMLElement>('[data-view="community-widgets"]')!,
  () => locale,
  refreshState,
  () => Boolean(state?.githubToken),
);

const previewRefresh = createPreviewRefresh(
  () => page === 'local-widgets' && !document.hidden,
  refreshAllPreviews,
);
const mainStartup = createMainStartup(
  () => invoke<boolean>('main_painted'),
  () => {
    widgetsReadyForPreviews = true;
    syncPreviewRefresh(true);
  },
);

const filterControl = createSegmentedControl(filterControlRoot, (value) => {
  selectFilter(value as WidgetFilter);
});
const themeControl = createSegmentedControl(themeControlRoot, (value) => {
  void setTheme(value as Theme);
});
const trayClickControl = createSegmentedControl(trayClickControlRoot, (value) => {
  void setTrayClickAction(value as TrayClickAction);
});
widgetSortButton.prepend(createSortIcon());
const localDetails = createLocalWidgetDetails(
  document.querySelector<HTMLElement>('[data-view="local-widgets"]')!,
  () => locale,
);

function t(key: MessageKey) {
  return translate(resolvedLocale(locale), key);
}

function applyLocaleStatic() {
  const resolved = resolvedLocale(locale);
  document.documentElement.lang = resolved === 'zh' ? 'zh-CN' : 'en';
  document.querySelectorAll<HTMLElement>('[data-message]').forEach((element) => {
    const key = element.dataset.message as MessageKey;
    element.textContent = t(key);
  });
  document.querySelector<HTMLElement>('nav')!.setAttribute('aria-label', t('mainNavigation'));
  themeControlRoot.setAttribute('aria-label', t('theme'));
  trayClickControlRoot.setAttribute('aria-label', t('trayClickAction'));
  filterOptions.setAttribute('aria-label', t('widgetFilters'));
  autoStart.setAttribute('aria-label', t('launchAtLogin'));
  githubTokenInput.placeholder = t('githubTokenPlaceholder');
  document.querySelectorAll<HTMLButtonElement>('[data-window-action]').forEach((button) => {
    button.setAttribute('aria-label', t(button.dataset.windowAction as MessageKey));
  });
  document.title = 'aidget';
}

function applyLocale() {
  applyLocaleStatic();
  if (state) {
    render(state);
  } else {
    renderPage();
  }
}

function renderPage() {
  if (page === 'local-widgets') {
    localDetails.render();
  } else {
    localDetails.close();
  }
  document.querySelectorAll<HTMLButtonElement>('[data-page]').forEach((button) => {
    const selected = button.dataset.page === page;
    button.classList.toggle('is-active', selected);
    if (selected) {
      button.setAttribute('aria-current', 'page');
    } else {
      button.removeAttribute('aria-current');
    }
  });
  document.querySelectorAll<HTMLElement>('[data-view]').forEach((view) => {
    view.hidden = view.dataset.view !== page;
  });
  currentGroup.textContent = t(pages[page].group);
  currentPage.textContent = t(pages[page].title);
  renderFilters();
  renderEmptyState();
  if (page === 'integrations') {
    ensureIntegrationInfo();
  }
  if (page === 'community-widgets') {
    community.show();
  } else {
    community.hide();
  }
  renderIntegration();
  renderDescription();
  filterControl.refresh();
  renderPreferences();
  renderGithubToken();
}

function renderDescription() {
  if (page !== 'integrations') {
    return;
  }
  renderMiddleEllipsis(pageDescription, t('mcpAndSkillsDescription'), 2);
}

function renderIntegration() {
  integrationPage.setAttribute('aria-busy', String(integrationBusy));
  if (!integrationInfo) {
    const message = t(integrationFailed ? 'integrationLoadFailed' : 'loading');
    mcpCommandOutput.textContent = message;
    skillDirectoryOutput.textContent = message;
    agentStrips.mcp.replaceChildren();
    agentStrips.skills.replaceChildren();
    renderIntegrationStatus();
    return;
  }
  mcpCommandOutput.textContent = mcpCommandLine(integrationInfo);
  skillDirectoryOutput.textContent = integrationInfo.skillDirectory;
  for (const resource of ['mcp', 'skills'] as const) {
    renderAgentStrip(
      agentStrips[resource],
      integrationInfo,
      resource,
      {
        all: t('agentAll'),
        empty: t('integrationNoAgents'),
        installFor: t('installFor'),
        removeFrom: t('removeFrom'),
      },
      integrationBusy,
      (agentIds, enabled) => void changeAgentIntegration(resource, agentIds, enabled),
    );
  }
  renderIntegrationStatus();
}

function ensureIntegrationInfo() {
  if (integrationInfo || integrationRequested) {
    return;
  }
  integrationRequested = true;
  void invoke<IntegrationInfo>('integration_state')
    .then((info) => {
      integrationInfo = info;
      integrationFailed = false;
      renderIntegration();
    })
    .catch((error: unknown) => {
      integrationFailed = true;
      renderIntegration();
      showFault(`${t('integrationLoadFailed')}: ${String(error)}`);
    });
}

function renderIntegrationStatus() {
  const message = integrationProblems.length
    ? integrationFailureMessage(
        integrationProblems.map((problem) => problem.agentId),
        integrationProblems.map((problem) => problem.message),
      )
    : '';
  integrationStatuses.forEach((status) => {
    status.textContent = message;
    status.classList.toggle('is-error', integrationProblems.length > 0);
    status.hidden = message.length === 0;
  });
}

function integrationFailureMessage(agentIds: string[], details: string[]) {
  const namesById = new Map(integrationInfo?.agents.map((agent) => [agent.id, agent.name]) ?? []);
  return formatAgentConfigurationFailure(
    resolvedLocale(locale),
    agentIds.map((id) => namesById.get(id) ?? id),
    details,
  );
}

async function changeAgentIntegration(
  resource: IntegrationResource,
  agentIds: string[],
  enabled: boolean,
) {
  if (integrationBusy || agentIds.length === 0) {
    return;
  }
  integrationBusy = true;
  integrationProblems = [];
  renderIntegration();
  fault.hidden = true;
  try {
    const result = await invoke<IntegrationMutation>('set_agent_integration', {
      change: { agentIds, resource, enabled },
    });
    integrationInfo = result.state;
    integrationProblems = result.problems;
  } catch (error) {
    showFault(integrationFailureMessage(agentIds, [String(error)]));
  } finally {
    integrationBusy = false;
    renderIntegration();
  }
}

function renderFilters() {
  filterControl.setValue(filter === 'deleted' ? null : filter);
  const deleted = filter === 'deleted';
  recycleFilter.classList.toggle('is-active', deleted);
  recycleFilter.setAttribute('aria-pressed', String(deleted));
  widgetSortButton.dataset.direction = widgetSort === 'oldest' ? 'asc' : 'desc';
  widgetSortLabel.textContent = t(widgetSort === 'newest' ? 'sortNewest' : 'sortOldest');
  widgetSortButton.setAttribute(
    'aria-label',
    t(widgetSort === 'newest' ? 'sortNewestDescription' : 'sortOldestDescription'),
  );
  widgetSortButton.title = widgetSortButton.getAttribute('aria-label') ?? '';
}

function selectFilter(next: WidgetFilter) {
  resetPurgeConfirmation();
  filter = next;
  if (state) {
    render(state);
  } else {
    renderPage();
  }
}

function renderEmptyState() {
  if (!state) {
    return;
  }
  if (filter === 'deleted') {
    emptyTitle.textContent = t('noDeletedWidgets');
    emptyDescription.textContent = t('noDeletedWidgetsDescription');
    return;
  }
  const noWidgets = isEmpty(state);
  emptyTitle.textContent = t(noWidgets ? 'noWidgets' : 'noMatchingWidgets');
  emptyDescription.textContent = t(
    noWidgets ? 'noWidgetsDescription' : 'noMatchingWidgetsDescription',
  );
}

function renderPreferences() {
  themeControl.setValue(theme);
  language.value = locale;
  trayClickControl.setValue(trayClickAction);
  trayLinuxNote.hidden = platform !== 'linux';
  themeControlRoot.querySelectorAll<HTMLButtonElement>('[data-value]').forEach((button) => {
    button.disabled = preferencesBusy;
  });
  language.disabled = preferencesBusy;
  trayClickControlRoot.querySelectorAll<HTMLButtonElement>('[data-value]').forEach((button) => {
    button.disabled = platform === 'linux' || preferencesBusy;
  });
}

function renderGithubToken() {
  const configured = state?.githubToken ?? null;
  const busy = githubTokenOperation !== undefined;
  githubTokenForm.hidden = configured !== null;
  githubTokenConfigured.hidden = configured === null;
  githubTokenInput.disabled = !state || busy;
  githubTokenSave.disabled = !state || busy;
  githubTokenRemove.disabled = busy;
  githubTokenControl.setAttribute('aria-busy', String(busy));
  githubTokenMasked.textContent = configured?.masked ?? '';
  githubTokenSave.textContent = t(githubTokenOperation === 'save' ? 'githubTokenSaving' : 'save');
  githubTokenRemove.textContent = t(
    githubTokenOperation === 'remove' ? 'githubTokenRemoving' : 'remove',
  );
  githubTokenError.textContent = githubTokenFailure;
  githubTokenError.hidden = githubTokenFailure.length === 0;
}

function currentPreferences(): PreferencesSnapshot {
  return { theme, language: locale, trayClickAction, lastPage: page };
}

function applyPreferences(next: PreferencesSnapshot) {
  const languageChanged = next.language !== locale;
  theme = next.theme;
  locale = next.language;
  trayClickAction = next.trayClickAction;
  applyTheme(theme);
  if (state) {
    state = {
      ...state,
      theme: next.theme,
      language: next.language,
      trayClickAction: next.trayClickAction,
    };
  }
  renderPreferences();
  if (languageChanged) {
    applyLocale();
  } else {
    renderPage();
  }
}

async function persistPreferences(previous: PreferencesSnapshot) {
  const next = currentPreferences();
  preferencesBusy = true;
  fault.hidden = true;
  renderPreferences();
  try {
    await queuePreferenceWrite(async () => {
      try {
        await invoke<void>('set_preferences', { preferences: next });
      } catch (error) {
        applyPreferences(previous);
        throw error;
      }
    });
  } catch (error) {
    showFault(`${t('settingFailed')}: ${String(error)}`);
  } finally {
    if (state) {
      state = { ...state, ...currentPreferences() };
    }
    preferencesBusy = false;
    renderPreferences();
  }
}

function queuePreferenceWrite(write: () => Promise<void>) {
  const queued = preferenceWrites.then(write);
  preferenceWrites = queued.catch(() => {});
  return queued;
}

async function rememberPage() {
  fault.hidden = true;
  try {
    await queuePreferenceWrite(() =>
      invoke<void>('set_preferences', { preferences: currentPreferences() }),
    );
  } catch (error) {
    showFault(`${t('settingFailed')}: ${String(error)}`);
  }
}

async function setTheme(next: Theme) {
  if (preferencesBusy || next === theme) {
    themeControl.setValue(theme);
    return;
  }
  const previous = currentPreferences();
  applyPreferences({ ...previous, theme: next });
  await persistPreferences(previous);
}

async function setLanguage(next: Locale) {
  if (preferencesBusy || next === locale) {
    language.value = locale;
    return;
  }
  const previous = currentPreferences();
  applyPreferences({ ...previous, language: next });
  await persistPreferences(previous);
}

async function setTrayClickAction(action: TrayClickAction) {
  if (platform === 'linux' || preferencesBusy || action === trayClickAction) {
    trayClickControl.setValue(trayClickAction);
    return;
  }
  const previous = currentPreferences();
  applyPreferences({ ...previous, trayClickAction: action });
  await persistPreferences(previous);
}

async function saveGithubToken() {
  if (!state || githubTokenOperation) {
    return;
  }
  const token = githubTokenInput.value.trim();
  if (!token) {
    githubTokenFailure = t('githubTokenRequired');
    renderGithubToken();
    githubTokenInput.focus();
    return;
  }
  githubTokenOperation = 'save';
  githubTokenFailure = '';
  renderGithubToken();
  try {
    const summary = await invoke<GithubTokenSummary | null>('set_github_token', { token });
    if (!summary) {
      throw new Error('GitHub token command returned no summary');
    }
    state = { ...state, githubToken: summary };
    githubTokenInput.value = '';
    community.invalidate(true);
  } catch (error) {
    githubTokenFailure = `${t('githubTokenSaveFailed')}: ${String(error)}`;
  } finally {
    githubTokenOperation = undefined;
    renderGithubToken();
    if (githubTokenFailure) {
      githubTokenInput.focus();
    }
  }
}

async function removeGithubToken() {
  if (!state?.githubToken || githubTokenOperation) {
    return;
  }
  githubTokenOperation = 'remove';
  githubTokenFailure = '';
  renderGithubToken();
  try {
    await invoke<null>('set_github_token', { token: null });
    state = { ...state, githubToken: null };
    githubTokenInput.value = '';
    community.invalidate(true);
  } catch (error) {
    githubTokenFailure = `${t('githubTokenRemoveFailed')}: ${String(error)}`;
  } finally {
    githubTokenOperation = undefined;
    renderGithubToken();
    if (state?.githubToken === null) {
      githubTokenInput.focus();
    }
  }
}

function createView(widget: WidgetSummary): WidgetView {
  const root = document.createElement('article');
  root.className = 'widget-card';
  root.dataset.widget = widget.id;

  const stage = document.createElement('button');
  stage.type = 'button';
  stage.className = 'preview-stage local-widget-preview';
  const image = document.createElement('img');
  image.className = 'preview-image';
  image.alt = '';
  image.width = Math.round(widget.width);
  image.height = Math.round(widget.height);
  image.hidden = true;
  const placeholder = document.createElement('span');
  placeholder.className = 'preview-placeholder';
  placeholder.textContent = t('previewUnavailable');
  stage.append(image, placeholder);

  const info = document.createElement('div');
  info.className = 'widget-info';
  const copy = document.createElement('div');
  copy.className = 'widget-copy';
  const title = document.createElement('button');
  title.type = 'button';
  title.className = 'widget-title local-widget-title';
  const size = document.createElement('p');
  size.className = 'widget-size';
  copy.append(title, size);

  const actions = document.createElement('div');
  actions.className = 'widget-actions';
  const toggle = document.createElement('button');
  toggle.className = 'switch';
  toggle.type = 'button';
  toggle.setAttribute('role', 'switch');
  toggle.addEventListener('click', () => void toggleWidget(view));
  const thumb = document.createElement('span');
  toggle.append(thumb);
  const archiveButton = document.createElement('button');
  archiveButton.className = 'widget-icon-button archive-button';
  archiveButton.type = 'button';
  archiveButton.append(
    createElement(Archive, {
      width: 16,
      height: 16,
      'stroke-width': 1.8,
      'aria-hidden': 'true',
    }),
  );
  archiveButton.addEventListener('click', () => void archiveWidget(view));
  actions.append(archiveButton, toggle);
  info.append(copy, actions);
  root.append(stage, info);

  const view = {
    root,
    stage,
    image,
    placeholder,
    title,
    size,
    toggle,
    archiveButton,
    widget,
    previewRequest: 0,
  };
  root.addEventListener('click', (event) => {
    if (event.target instanceof Element && event.target.closest('.widget-actions')) {
      return;
    }
    void localDetails.show(view.widget.id).catch((error: unknown) => {
      showFault(`${t('widgetDetailsFailed')}: ${String(error)}`);
    });
  });
  updateView(view, widget);
  return view;
}

function updateView(view: WidgetView, widget: WidgetSummary) {
  view.stage.setAttribute('aria-label', `${t('communityDetails')}: ${widget.title || widget.id}`);
  view.placeholder.textContent = t('previewUnavailable');
  view.title.textContent = widget.title || widget.id;
  view.size.textContent = `${Math.round(widget.width)} × ${Math.round(widget.height)}`;
  view.image.width = Math.round(widget.width);
  view.image.height = Math.round(widget.height);
  view.toggle.setAttribute('aria-checked', String(widget.enabled));
  view.toggle.setAttribute('aria-label', t(widget.enabled ? 'disable' : 'enable'));
  view.archiveButton.setAttribute('aria-label', `${t('archive')} ${widget.title || widget.id}`);
  view.archiveButton.title = t('archive');
  view.root.dataset.enabled = String(widget.enabled);
  view.widget = widget;
}

function createDeletedView(widget: DeletedWidgetSummary): DeletedWidgetView {
  const root = document.createElement('article');
  root.className = 'widget-card deleted-card';
  root.dataset.deleted = widget.trashName;

  const stage = document.createElement('div');
  stage.className = 'preview-stage';
  const image = document.createElement('img');
  image.className = 'preview-image';
  image.alt = '';
  image.width = Math.round(widget.width);
  image.height = Math.round(widget.height);
  image.hidden = true;
  const placeholder = document.createElement('span');
  placeholder.className = 'preview-placeholder';
  placeholder.textContent = t('previewUnavailable');
  stage.append(image, placeholder);

  const info = document.createElement('div');
  info.className = 'widget-info';
  const copy = document.createElement('div');
  copy.className = 'widget-copy';
  const title = document.createElement('h2');
  title.className = 'widget-title';
  const size = document.createElement('p');
  size.className = 'widget-size';
  copy.append(title, size);

  const actions = document.createElement('div');
  actions.className = 'widget-actions';
  const restoreButton = document.createElement('button');
  restoreButton.className = 'widget-icon-button restore-button';
  restoreButton.type = 'button';
  restoreButton.append(
    createElement(RotateCcw, {
      width: 16,
      height: 16,
      'stroke-width': 1.8,
      'aria-hidden': 'true',
    }),
  );
  const deleteButton = document.createElement('button');
  deleteButton.className = 'widget-icon-button purge-button';
  deleteButton.type = 'button';
  deleteButton.append(
    createElement(Trash, {
      width: 16,
      height: 16,
      'stroke-width': 1.8,
      'aria-hidden': 'true',
    }),
  );
  const confirmLabel = document.createElement('span');
  confirmLabel.className = 'purge-confirm-label';
  deleteButton.append(confirmLabel);
  actions.append(restoreButton, deleteButton);
  info.append(copy, actions);
  root.append(stage, info);

  const view = {
    root,
    image,
    placeholder,
    title,
    size,
    restoreButton,
    deleteButton,
    trashName: widget.trashName,
  };
  restoreButton.addEventListener('click', () => void restoreDeletedWidget(view));
  deleteButton.addEventListener('click', () => armPurgeDeletedWidget(view));
  updateDeletedView(view, widget);
  if (widget.hasPreview) {
    void loadDeletedPreview(view, widget);
  }
  return view;
}

function updateDeletedView(view: DeletedWidgetView, widget: DeletedWidgetSummary) {
  view.placeholder.textContent = t('previewUnavailable');
  view.title.textContent = widget.title || widget.id;
  view.size.textContent = `${Math.round(widget.width)} × ${Math.round(widget.height)}`;
  view.restoreButton.setAttribute('aria-label', `${t('restore')} ${widget.title || widget.id}`);
  view.restoreButton.title = t('restore');
  view.deleteButton.dataset.widgetTitle = widget.title || widget.id;
  view.deleteButton.querySelector<HTMLElement>('.purge-confirm-label')!.textContent =
    t('confirmDelete');
  view.trashName = widget.trashName;
  syncPurgeButton(view, pendingPurge === view);
}

function renderDeletedWidgets(widgets: DeletedWidgetSummary[]) {
  const sorted = sortedDeletedWidgets(widgets, widgetSort);
  const alive = new Set(sorted.map((widget) => widget.trashName));
  if (pendingPurge && !alive.has(pendingPurge.trashName)) {
    resetPurgeConfirmation();
  }
  for (const [trashName, view] of deletedViews) {
    if (alive.has(trashName)) {
      continue;
    }
    view.root.remove();
    deletedViews.delete(trashName);
  }
  for (const widget of sorted) {
    const view = deletedViews.get(widget.trashName) ?? createDeletedView(widget);
    deletedViews.set(widget.trashName, view);
    updateDeletedView(view, widget);
  }
  const ordered = sorted.map((widget) => deletedViews.get(widget.trashName)!.root);
  if (ordered.some((node, index) => node !== deletedGrid.children[index])) {
    deletedGrid.append(...ordered);
  }
}

async function loadDeletedPreview(view: DeletedWidgetView, widget: DeletedWidgetSummary) {
  try {
    const preview = await invoke<string | null>('deleted_widget_preview', {
      trashName: widget.trashName,
    });
    if (view.trashName !== widget.trashName) {
      return;
    }
    view.image.hidden = preview === null;
    view.placeholder.hidden = preview !== null;
    if (preview !== null) {
      view.image.src = preview;
    }
  } catch (error) {
    view.image.hidden = true;
    view.placeholder.hidden = false;
    showFault(`${t('previewUnavailable')}: ${widget.title || widget.id} (${String(error)})`);
  }
}

async function loadPreview(view: WidgetView, widget: WidgetSummary) {
  const request = ++view.previewRequest;
  const key = previewKey(widget);
  try {
    const preview = await invoke<string | null>('main_widget_preview', { id: widget.id });
    if (
      request !== view.previewRequest ||
      view.widget.id !== widget.id ||
      previewKey(view.widget) !== key
    ) {
      return;
    }
    view.image.hidden = preview === null;
    view.placeholder.hidden = preview !== null;
    if (preview !== null) {
      view.image.src = preview;
    }
  } catch (error) {
    if (request !== view.previewRequest || views.get(widget.id) !== view || !view.widget.enabled) {
      return;
    }
    showFault(`${t('previewUnavailable')}: ${widget.title || widget.id} (${String(error)})`);
  }
}

async function loadCachedPreview(view: WidgetView, widget: WidgetSummary) {
  const request = ++view.previewRequest;
  const key = previewKey(widget);
  try {
    const preview = await invoke<string | null>('main_widget_cached_preview', {
      id: widget.id,
    });
    if (
      request !== view.previewRequest ||
      view.widget.id !== widget.id ||
      previewKey(view.widget) !== key
    ) {
      return;
    }
    view.image.hidden = preview === null;
    view.placeholder.hidden = preview !== null;
    if (preview !== null) {
      view.image.src = preview;
    }
  } catch (error) {
    if (request !== view.previewRequest || views.get(widget.id) !== view) {
      return;
    }
    showFault(`${t('previewUnavailable')}: ${widget.title || widget.id} (${String(error)})`);
  }
}

async function refreshAllPreviews() {
  const current = state;
  if (!current || page !== 'local-widgets' || document.hidden) {
    return;
  }
  const widgets = filteredWidgets(sortedActiveWidgets(current.widgets, widgetSort), 'enabled');
  for (const widget of widgets) {
    if (page !== 'local-widgets' || document.hidden) {
      return;
    }
    const view = views.get(widget.id);
    if (view) {
      await loadPreview(view, widget);
    }
  }
}

function syncPreviewRefresh(immediate = false) {
  if (!widgetsReadyForPreviews) {
    return;
  }
  const active = page === 'local-widgets' && !document.hidden;
  if (!active) {
    previewRefresh.stop();
    previewRefreshActive = false;
    return;
  }
  const resumed = immediate && !previewRefreshActive;
  previewRefresh.start();
  previewRefreshActive = true;
  if (resumed) {
    void previewRefresh.refreshNow();
  }
}

function render(next: MainState) {
  const initial = state === undefined;
  if (initial) {
    page = next.lastPage;
  }
  const languageChanged = next.language !== locale;
  if (!preferencesBusy) {
    theme = next.theme;
    locale = next.language;
    trayClickAction = next.trayClickAction;
    applyTheme(theme);
    if (languageChanged) {
      applyLocaleStatic();
    }
  }
  const previous = new Map([...views].map(([id, view]) => [id, view.widget]));
  const widgets = sortedActiveWidgets(next.widgets, widgetSort);
  const deletedMode = filter === 'deleted';
  const visibleWidgets = deletedMode ? [] : filteredWidgets(widgets, filter);
  const visibleIds = new Set(visibleWidgets.map((widget) => widget.id));
  const alive = new Set(widgets.map((widget) => widget.id));
  for (const [id, view] of views) {
    if (alive.has(id)) {
      continue;
    }
    view.root.remove();
    views.delete(id);
  }
  for (const widget of widgets) {
    const view = views.get(widget.id) ?? createView(widget);
    views.set(widget.id, view);
    updateView(view, widget);
    view.root.hidden = !visibleIds.has(widget.id);
    if (!previous.has(widget.id) || previewNeedsRefresh(previous.get(widget.id), widget)) {
      if (initial) {
        void loadCachedPreview(view, widget);
      } else {
        void loadPreview(view, widget);
      }
    }
  }
  const ordered = widgets.map((widget) => views.get(widget.id)!.root);
  if (ordered.some((node, index) => node !== grid.children[index])) {
    grid.append(...ordered);
  }
  renderDeletedWidgets(next.deletedWidgets);
  const membershipChanged =
    state?.widgets.map((w) => `${w.id}:${w.packageName}`).join() !==
    next.widgets.map((w) => `${w.id}:${w.packageName}`).join();
  if (membershipChanged) {
    community.invalidate();
  }
  if (state?.githubToken?.masked !== next.githubToken?.masked) {
    community.invalidate(true);
  }
  state = next;
  if (next.broken.length) {
    if (fault.hidden || faultSource === 'broken') {
      showFault(next.broken.map((item) => `${item.id}: ${item.error}`).join('\n'), 'broken');
    }
  } else if (faultSource === 'broken') {
    fault.hidden = true;
    fault.textContent = '';
    faultSource = undefined;
  }
  version.textContent = next.version;
  empty.hidden = deletedMode ? next.deletedWidgets.length > 0 : visibleWidgets.length > 0;
  grid.hidden = deletedMode || visibleWidgets.length === 0;
  deletedGrid.hidden = !deletedMode || next.deletedWidgets.length === 0;
  renderPage();
}

function showFault(message: string, source: 'operation' | 'broken' = 'operation') {
  faultSource = source;
  fault.textContent = message;
  fault.hidden = false;
}

async function refreshState() {
  try {
    render(await invoke<MainState>('main_state'));
  } catch (error) {
    showFault(`${t('loadFailed')}: ${String(error)}`);
  }
}

async function toggleWidget(view: WidgetView) {
  view.toggle.disabled = true;
  fault.hidden = true;
  try {
    await invoke('set_widget_enabled', {
      id: view.widget.id,
      enabled: !view.widget.enabled,
    });
  } catch (error) {
    showFault(`${t('settingFailed')}: ${String(error)}`);
  } finally {
    view.toggle.disabled = false;
  }
}

async function archiveWidget(view: WidgetView) {
  view.archiveButton.disabled = true;
  fault.hidden = true;
  try {
    await invoke('delete_widget', { id: view.widget.id });
  } catch (error) {
    showFault(`${t('settingFailed')}: ${String(error)}`);
    view.archiveButton.disabled = false;
  }
}

async function restoreDeletedWidget(view: DeletedWidgetView) {
  if (pendingPurge === view) {
    resetPurgeConfirmation();
  }
  view.restoreButton.disabled = true;
  fault.hidden = true;
  try {
    await invoke('restore_widget', { trashName: view.trashName });
  } catch (error) {
    showFault(`${t('restoreFailed')}: ${String(error)}`);
    view.restoreButton.disabled = false;
  }
}

function syncPurgeButton(view: DeletedWidgetView, pending: boolean) {
  const title = view.deleteButton.dataset.widgetTitle ?? '';
  view.deleteButton.dataset.confirm = String(pending);
  view.deleteButton.setAttribute(
    'aria-label',
    `${t(pending ? 'confirmDelete' : 'deletePermanently')} ${title}`,
  );
  view.deleteButton.title = t(pending ? 'confirmDelete' : 'deletePermanently');
}

function resetPurgeConfirmation() {
  if (purgeTimer !== undefined) {
    window.clearTimeout(purgeTimer);
  }
  purgeTimer = undefined;
  if (!pendingPurge) {
    return;
  }
  syncPurgeButton(pendingPurge, false);
  pendingPurge = undefined;
}

function armPurgeDeletedWidget(view: DeletedWidgetView) {
  if (view.deleteButton.disabled) {
    return;
  }
  if (pendingPurge === view) {
    void purgeDeletedWidget(view);
    return;
  }
  resetPurgeConfirmation();
  pendingPurge = view;
  syncPurgeButton(view, true);
  purgeTimer = window.setTimeout(resetPurgeConfirmation, 3000);
}

async function purgeDeletedWidget(view: DeletedWidgetView) {
  resetPurgeConfirmation();
  view.deleteButton.disabled = true;
  fault.hidden = true;
  try {
    await invoke('purge_deleted_widget', { trashName: view.trashName });
  } catch (error) {
    showFault(`${t('deleteFailed')}: ${String(error)}`);
    view.deleteButton.disabled = false;
  }
}

async function initializeAutostart() {
  autoStart.setAttribute('aria-checked', String(await isEnabled()));
}

autoStart.addEventListener('click', async () => {
  autoStart.disabled = true;
  fault.hidden = true;
  try {
    if (autoStart.getAttribute('aria-checked') === 'true') {
      await disable();
    } else {
      await enable();
    }
    await initializeAutostart();
  } catch (error) {
    showFault(`${t('startupFailed')}: ${String(error)}`);
  } finally {
    autoStart.disabled = false;
  }
});

githubButton.addEventListener('click', () => {
  fault.hidden = true;
  void openUrl(GITHUB_REPOSITORY_URL).catch((error: unknown) => {
    showFault(`${t('openLinkFailed')}: ${String(error)}`);
  });
});

document.querySelectorAll<HTMLButtonElement>('[data-page]').forEach((button) => {
  button.addEventListener('click', () => {
    resetPurgeConfirmation();
    page = button.dataset.page as MainPage;
    if (page === 'integrations') {
      integrationRequested = false;
      integrationFailed = false;
      integrationInfo = undefined;
      integrationProblems = [];
    }
    fault.hidden = true;
    renderPage();
    syncPreviewRefresh(true);
    void rememberPage();
  });
});

recycleFilter.addEventListener('click', () => {
  selectFilter('deleted');
});

widgetSortButton.addEventListener('click', () => {
  resetPurgeConfirmation();
  widgetSort = widgetSort === 'newest' ? 'oldest' : 'newest';
  if (state) {
    render(state);
  } else {
    renderPage();
  }
});

document.addEventListener(
  'pointerdown',
  (event) => {
    if (!pendingPurge) {
      return;
    }
    if (pendingPurge.deleteButton.contains(event.target as Node)) {
      return;
    }
    resetPurgeConfirmation();
  },
  true,
);

document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape' && pendingPurge) {
    resetPurgeConfirmation();
  }
});

window.addEventListener('resize', renderDescription);
document.addEventListener('visibilitychange', () => syncPreviewRefresh(true));

language.addEventListener('change', () => {
  void setLanguage(language.value as Locale);
});
githubTokenForm.addEventListener('submit', (event) => {
  event.preventDefault();
  void saveGithubToken();
});
githubTokenInput.addEventListener('input', () => {
  if (!githubTokenFailure) {
    return;
  }
  githubTokenFailure = '';
  renderGithubToken();
});
githubTokenRemove.addEventListener('click', () => {
  void removeGithubToken();
});

document.querySelectorAll<HTMLElement>('[data-drag-region]').forEach((region) => {
  region.addEventListener('pointerdown', (event) => {
    if (event.button !== 0 || (event.target as Element).closest('button')) {
      return;
    }
    void windowApi.startDragging();
  });
});

document.querySelectorAll<HTMLButtonElement>('[data-window-action]').forEach((button) => {
  button.addEventListener('click', () => {
    const action = button.dataset.windowAction;
    const command =
      action === 'minimize'
        ? windowApi.minimize()
        : action === 'maximize'
          ? windowApi.toggleMaximize()
          : windowApi.close();
    void command.catch((error: unknown) => showFault(String(error)));
  });
});

autoStart.setAttribute('aria-checked', 'false');

brandMark.src = brandIcon;
createIcons({
  icons: { Info, LayoutGrid, Minus, ServerCog, Settings, Square, UsersRound, X },
});
applyTheme(theme);
renderPreferences();
applyLocaleStatic();
renderGithubToken();
void initializeAutostart().catch((error: unknown) =>
  showFault(`${t('startupFailed')}: ${String(error)}`),
);
void listen<MainState>('aidget://main', (event) => render(event.payload));
void listen('aidget://widgets-ready', () => mainStartup.synchronized());

void (async () => {
  await refreshState();
  await mainStartup.painted();
})().catch((error: unknown) => {
  showFault(`${t('startupFailed')}: ${String(error)}`);
});
