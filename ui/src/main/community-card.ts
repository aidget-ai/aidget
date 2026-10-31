import { createElement, LayoutGrid, ShieldCheck } from 'lucide';

import type { CommunityEntry } from '../types/rust';
import { createCommunityMedia } from './community-media';
import { translate, type Locale } from './locale';
import { resolvedLocale } from './preferences';

/** Maintain one catalogue card, retaining decoded media while its digest remains unchanged. */
export function createCommunityCard(
  entry: CommunityEntry,
  commit: string,
  media: ReturnType<typeof createCommunityMedia>,
  scrollRoot: Element,
  locale: () => Locale,
  details: (entry: CommunityEntry, commit: string) => void,
) {
  const t = (key: Parameters<typeof translate>[1]) => translate(resolvedLocale(locale()), key);
  let revision = commit;
  const element = document.createElement('article');
  element.className = 'widget-card community-card';
  element.dataset.package = entry.entry.package.name;
  const top = document.createElement('div');
  top.className = 'widget-info community-card-heading';
  const logoBox = document.createElement('div');
  const placeholder = document.createElement('span');
  placeholder.className = 'community-logo community-logo-default';
  placeholder.setAttribute('aria-hidden', 'true');
  placeholder.append(createElement(LayoutGrid, { width: 24, height: 24, 'stroke-width': 1.6 }));
  logoBox.append(placeholder);
  const identity = document.createElement('div');
  identity.className = 'community-identity';
  const name = document.createElement('button');
  name.type = 'button';
  name.className = 'widget-title community-name';
  name.addEventListener('click', () => details(entry, revision));
  const by = document.createElement('span');
  by.className = 'community-verified';
  const verified = document.createElement('span');
  by.append(
    createElement(ShieldCheck, {
      width: 13,
      height: 13,
      'stroke-width': 1.7,
      'aria-hidden': 'true',
    }),
    verified,
  );
  identity.append(name, by);
  const install = document.createElement('button');
  install.type = 'button';
  install.className = 'community-install secondary-button';
  install.addEventListener('click', () => details(entry, revision));
  top.append(logoBox, identity, install);
  const preview = document.createElement('button');
  preview.type = 'button';
  preview.className = 'community-preview-button';
  preview.addEventListener('click', () => details(entry, revision));
  const screenshot = media.create(entry, entry.entry.config.screenshots[0], commit, scrollRoot);
  preview.append(screenshot.stage);
  screenshot.element.prepend(preview);
  const footer = document.createElement('div');
  footer.className = 'community-card-footer';
  const count = document.createElement('span');
  count.dataset.downloadCount = entry.entry.package.name;
  const version = document.createElement('span');
  footer.append(count, version);
  element.append(top, screenshot.element, footer);
  let logo: ReturnType<typeof media.create> | undefined;

  function update(next: CommunityEntry, commit: string, installing: boolean, label: string) {
    entry = next;
    revision = commit;
    const { config, package: pkg } = entry.entry;
    name.textContent = config.title;
    name.title = config.title;
    name.disabled = installing;
    verified.textContent = t('communityVerified');
    by.title = typeof pkg.author === 'string' ? pkg.author : (pkg.author?.name ?? '');
    install.textContent = label;
    install.disabled = installing || entry.installed || entry.idConflict || !entry.supported;
    preview.disabled = installing;
    preview.setAttribute('aria-label', `${t('communityDetails')}: ${config.title}`);
    screenshot.update(entry, config.screenshots[0], commit);
    if (config.logo) {
      if (logo) {
        logo.update(entry, config.logo, commit);
      } else {
        logo = media.create(entry, config.logo, commit, scrollRoot, false, true);
        logoBox.replaceChildren(logo.element);
      }
    } else if (logo) {
      logo.dispose();
      logo = undefined;
      logoBox.replaceChildren(placeholder);
    }
    version.textContent = `v${pkg.version}`;
  }
  return {
    element,
    count,
    update,
    dispose() {
      screenshot.dispose();
      logo?.dispose();
      element.remove();
    },
  };
}
