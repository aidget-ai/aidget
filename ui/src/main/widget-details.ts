import { invoke } from '@tauri-apps/api/core';

import type {
  HostCompatibility,
  LocalWidgetDetails,
  WidgetConfig,
  WidgetPermission,
  WidgetPlatform,
} from '../types/rust';
import type { Language } from '../types/rust';
import { closeDialog, openDialog, setupDialogAnimation } from '../utils/dialog';
import { translate, type MessageKey } from './locale';

const platformNames: Record<WidgetPlatform, string> = {
  macos: 'macOS',
  windows: 'Windows',
  linux: 'Linux',
};
const permissionNames: Record<WidgetPermission, MessageKey> = {
  script: 'permissionScript',
  location: 'permissionLocation',
  storage: 'permissionStorage',
};

/** Render the shared preview, description and declared capabilities for widget details. */
export function renderWidgetDetailsContent(
  config: WidgetConfig,
  compatibility: HostCompatibility,
  description: string | null,
  previews: HTMLElement[],
  locale: Language,
  onUpdate?: () => void,
) {
  const t = (key: MessageKey) => translate(locale, key);
  const content = document.createElement('div');
  content.className = 'community-dialog-content';
  const images = document.createElement('div');
  images.className = 'community-detail-images';
  images.append(...previews);
  content.append(images);
  const hostTitle = document.createElement('h3');
  hostTitle.textContent = t('communityHostVersion');
  const hostVersion = document.createElement('p');
  hostVersion.className = 'community-host-version';
  hostVersion.textContent =
    compatibility.minHostVersion === null
      ? t('communityHostUndeclared')
      : compatibility.minHostVersion;
  if (compatibility.state === 'host_too_old') {
    const warning = document.createElement('p');
    warning.className = 'community-host-incompatible';
    warning.append(document.createTextNode(t('communityHostUnsupportedBefore')));
    const update = document.createElement('button');
    update.type = 'button';
    update.className = 'text-button';
    update.textContent = t('communityHostUpdate');
    update.addEventListener('click', () => onUpdate?.());
    warning.append(update, document.createTextNode(t('communityHostUnsupportedAfter')));
    hostVersion.append(' · ', warning);
  }
  if (description) {
    const text = document.createElement('p');
    text.className = 'community-detail-description';
    text.textContent = description;
    content.append(text);
  }
  const platformsTitle = document.createElement('h3');
  platformsTitle.textContent = t('communityPlatforms');
  const platforms = document.createElement('ul');
  platforms.className = 'community-badges community-platforms';
  for (const platform of config.platforms) {
    const item = document.createElement('li');
    item.textContent = platformNames[platform];
    platforms.append(item);
  }
  const permissionsTitle = document.createElement('h3');
  permissionsTitle.textContent = t('communityPermissions');
  const permissions = document.createElement(config.permissions.length ? 'ul' : 'p');
  permissions.className = config.permissions.length
    ? 'community-badges community-permissions'
    : 'community-permissions-empty';
  if (config.permissions.length) {
    for (const permission of config.permissions) {
      const item = document.createElement('li');
      item.textContent = t(permissionNames[permission]);
      permissions.append(item);
    }
  } else {
    permissions.textContent = t('communityNoPermissions');
  }
  const networkTitle = document.createElement('h3');
  networkTitle.textContent = t('communityNetwork');
  const network = document.createElement(config.network.length ? 'ul' : 'p');
  network.className = config.network.length ? 'community-network' : 'community-network-empty';
  if (config.network.length) {
    for (const origin of config.network) {
      const item = document.createElement('li');
      item.textContent = origin;
      network.append(item);
    }
  } else {
    network.textContent = t('communityNoNetwork');
  }
  content.append(
    platformsTitle,
    platforms,
    permissionsTitle,
    permissions,
    networkTitle,
    network,
    hostTitle,
    hostVersion,
  );
  return content;
}

/** Display installed metadata without community identity or an installation action. */
export function createLocalWidgetDetails(
  root: HTMLElement,
  locale: () => Language,
  onUpdate: () => void,
) {
  let details: LocalWidgetDetails | undefined;
  let generation = 0;
  const dialog = document.createElement('dialog');
  dialog.className = 'community-dialog local-widget-dialog';
  dialog.tabIndex = -1;
  dialog.setAttribute('aria-labelledby', 'local-widget-detail-title');
  setupDialogAnimation(dialog);
  dialog.addEventListener('close', () => {
    generation++;
    details = undefined;
  });
  root.append(dialog);

  function render() {
    if (!details) {
      return;
    }
    const t = (key: MessageKey) => translate(locale(), key);
    const title = document.createElement('h2');
    title.id = 'local-widget-detail-title';
    title.textContent = details.config.title;
    const header = document.createElement('div');
    header.className = 'community-dialog-header';
    header.append(title);
    const preview = document.createElement('div');
    preview.className = 'preview-stage community-preview';
    if (details.preview !== null) {
      const image = document.createElement('img');
      image.className = 'preview-image';
      image.src = details.preview;
      image.alt = details.config.title;
      preview.append(image);
    } else {
      preview.textContent = t('previewUnavailable');
    }
    const content = renderWidgetDetailsContent(
      details.config,
      details.compatibility,
      details.description,
      [preview],
      locale(),
      async () => {
        await closeDialog(dialog);
        onUpdate();
      },
    );
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'community-dialog-close';
    close.textContent = t('communityClose');
    close.addEventListener('click', () => void closeDialog(dialog));
    const footer = document.createElement('div');
    footer.className = 'community-dialog-footer';
    footer.append(close);
    dialog.replaceChildren(header, content, footer);
  }
  return {
    /** Read details on demand; only the most recently requested widget may open the dialog. */
    async show(id: string) {
      const current = ++generation;
      const next = await invoke<LocalWidgetDetails>('main_widget_details', { id });
      if (current !== generation) {
        return;
      }
      details = next;
      render();
      openDialog(dialog);
    },
    /** Dismiss the dialog and cancel pending openings when leaving the local panel. */
    close() {
      generation++;
      details = undefined;
      if (dialog.open) {
        void closeDialog(dialog);
      }
    },
    /** Update an open dialog after a language change. */
    render,
  };
}
