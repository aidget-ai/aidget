import type { DisplayInfo } from '../types/rust';

export interface DisplaySelectLabels {
  automatic: string;
  primary: string;
  disconnected: string;
}

// Select values are strings, so the empty value stands for no chosen display.
const AUTOMATIC = '';

/**
 * Find a display by its stored name. `primary` follows the system's primary display,
 * or the first one where none is reported, as the Rust side does.
 */
export function findDisplay(displays: DisplayInfo[], name: string) {
  if (name === 'primary') {
    return displays.find((display) => display.primary) ?? displays[0];
  }
  return displays.find((display) => display.name === name);
}

function displayLabel(display: DisplayInfo, labels: DisplaySelectLabels) {
  const parts = [display.name, `${Math.round(display.width)} × ${Math.round(display.height)}`];
  if (display.primary) {
    parts.push(labels.primary);
  }
  return parts.join(' · ');
}

/** Fill a display menu, keeping a disconnected saved display selected. */
export function renderDisplaySelect(
  select: HTMLSelectElement,
  displays: DisplayInfo[],
  selected: string | null,
  labels: DisplaySelectLabels,
) {
  const current = selected === null ? undefined : findDisplay(displays, selected);
  const options = [
    [AUTOMATIC, labels.automatic],
    ...displays.map((display) => [display.name, displayLabel(display, labels)]),
  ];
  if (selected !== null && !current) {
    options.push([selected, `${selected} · ${labels.disconnected}`]);
  }
  const signature = JSON.stringify(options);
  // Main-state events are frequent, and rebuilding would close an open dropdown.
  if (select.dataset.options !== signature) {
    select.replaceChildren(
      ...options.map(([value, label]) => {
        const option = document.createElement('option');
        option.value = value;
        option.textContent = label;
        return option;
      }),
    );
    select.dataset.options = signature;
  }
  select.value = current?.name ?? selected ?? AUTOMATIC;
  select.title = select.selectedOptions[0]?.textContent ?? '';
}

/** Read the menu back as the value stored in preferences and frames. */
export function selectedDisplay(select: HTMLSelectElement) {
  return select.value === AUTOMATIC ? null : select.value;
}
