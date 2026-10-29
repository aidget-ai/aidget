import { invoke } from '@tauri-apps/api/core';

import type {
  LocalWidgetDetails,
  WidgetConfig,
  WidgetPermission,
  WidgetPlatform,
} from '../types/rust';
import { translate, type Locale, type MessageKey } from './locale';
import { resolvedLocale } from './preferences';

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
  description: string | null,
  previews: HTMLElement[],
  locale: Locale,
) {
  const t = (key: MessageKey) => translate(resolvedLocale(locale), key);
  const content = document.createElement('div');
  content.className = 'community-dialog-content';
  const images = document.createElement('div');
  images.className = 'community-detail-images';
  images.append(...previews);
  content.append(images);
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
  content.append(platformsTitle, platforms, permissionsTitle, permissions, networkTitle, network);
  return content;
}

/** Display installed metadata without community identity or an installation action. */
export function createLocalWidgetDetails(root: HTMLElement, locale: () => Locale) {
  let details: LocalWidgetDetails | undefined;
  let generation = 0;
  const dialog = document.createElement('dialog');
  dialog.className = 'community-dialog local-widget-dialog';
  dialog.tabIndex = -1;
  dialog.setAttribute('aria-labelledby', 'local-widget-detail-title');
  dialog.addEventListener('close', () => {
    generation++;
    details = undefined;
  });
  root.append(dialog);

  function render() {
    if (!details) {
      return;
    }
    const t = (key: MessageKey) => translate(resolvedLocale(locale()), key);
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
      details.description,
      [preview],
      locale(),
    );
    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'community-dialog-close';
    close.textContent = t('communityClose');
    close.addEventListener('click', () => dialog.close());
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
      if (!dialog.open) {
        dialog.showModal();
      }
      dialog.focus();
    },
    /** Dismiss the dialog and cancel pending openings when leaving the local panel. */
    close() {
      generation++;
      details = undefined;
      if (dialog.open) {
        dialog.close();
      }
    },
    /** Update an open dialog after a language change. */
    render,
  };
}
