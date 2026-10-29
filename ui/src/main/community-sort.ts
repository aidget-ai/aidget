import { translate, type Locale, type MessageKey } from './locale';
import { resolvedLocale } from './preferences';
import { createSegmentedControl } from './segmented-control';
import { createSortIcon } from './sort-icon';

/** The active catalogue metric and its ordering direction. */
export type CommunitySort = `${'downloads' | 'published'}-${'asc' | 'desc'}`;

/** Select a catalogue metric in a segmented control; repeat its activation to reverse ordering. */
export function createCommunitySort(locale: () => Locale, changed: () => void) {
  let value: CommunitySort = 'downloads-desc';
  const t = (key: MessageKey) => translate(resolvedLocale(locale()), key);
  const element = document.createElement('div');
  element.className = 'community-sort segmented-control';
  element.setAttribute('role', 'group');
  const indicator = document.createElement('span');
  indicator.className = 'segmented-control-indicator';
  indicator.setAttribute('aria-hidden', 'true');
  element.append(indicator);
  const rows = (['downloads', 'published'] as const).map((metric) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'segment-button';
    button.dataset.sort = metric;
    button.dataset.value = metric;
    const text = document.createElement('span');
    button.append(createSortIcon(), text);
    element.append(button);
    return { metric, button, text };
  });
  const control = createSegmentedControl(element, (metric) => {
    if (metric !== 'downloads' && metric !== 'published') {
      throw new Error(`unknown community sort metric: ${metric}`);
    }
    // New metrics start descending; activating the current metric reverses it.
    value = value === `${metric}-desc` ? `${metric}-asc` : `${metric}-desc`;
    render();
    changed();
  });

  function render(disabled = false) {
    element.setAttribute('aria-label', t('communitySort'));
    control.setValue(value.startsWith('downloads') ? 'downloads' : 'published');
    for (const row of rows) {
      const active = value.startsWith(row.metric);
      row.text.textContent = t(
        row.metric === 'downloads' ? 'communitySortDownloads' : 'communitySortPublished',
      );
      row.button.dataset.direction = active ? (value.endsWith('asc') ? 'asc' : 'desc') : '';
      const direction: MessageKey =
        row.metric === 'downloads'
          ? active && value.endsWith('asc')
            ? 'communitySortDownloadsAsc'
            : 'communitySortDownloadsDesc'
          : active && value.endsWith('asc')
            ? 'communitySortPublishedAsc'
            : 'communitySortPublishedDesc';
      row.button.title = t(direction);
      row.button.disabled = disabled;
    }
    control.refresh();
  }
  render();
  return {
    element,
    get value() {
      return value;
    },
    /** Update translations and availability without resetting the selected ordering. */
    render,
  };
}
