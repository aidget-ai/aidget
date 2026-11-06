/// @vitest-environment happy-dom

import { describe, expect, it } from 'vite-plus/test';

import { findDisplay, renderDisplaySelect, selectedDisplay } from '../../src/main/display-select';
import type { DisplayInfo } from '../../src/types/rust';

const labels = { automatic: 'Automatic', primary: 'Primary', disconnected: 'Disconnected' };
const builtIn: DisplayInfo = {
  name: 'Monitor #1',
  primary: true,
  x: 0,
  y: 33,
  width: 1470,
  height: 859,
  scale: 2,
};
const external: DisplayInfo = {
  name: 'Monitor #2',
  primary: false,
  x: -719,
  y: -1410,
  width: 2560,
  height: 1410,
  scale: 2,
};

function optionTexts(select: HTMLSelectElement) {
  return [...select.options].map((option) => option.textContent);
}

describe('display select', () => {
  it('offers automatic placement and every connected display with its size', () => {
    const select = document.createElement('select');

    renderDisplaySelect(select, [builtIn, external], null, labels);

    expect(optionTexts(select)).toEqual([
      'Automatic',
      'Monitor #1 · 1470 × 859 · Primary',
      'Monitor #2 · 2560 × 1410',
    ]);
    expect(selectedDisplay(select)).toBeNull();
  });

  it('selects the primary display for a frame stored as primary', () => {
    const select = document.createElement('select');

    renderDisplaySelect(select, [builtIn, external], 'primary', labels);

    expect(selectedDisplay(select)).toBe('Monitor #1');
  });

  it('exposes the selected label so a truncated menu can still be read', () => {
    const select = document.createElement('select');

    renderDisplaySelect(select, [builtIn, external], 'Monitor #1', labels);

    expect(select.title).toBe('Monitor #1 · 1470 × 859 · Primary');
  });

  it('keeps a disconnected saved display selected instead of showing automatic', () => {
    const select = document.createElement('select');

    renderDisplaySelect(select, [builtIn], 'Monitor #2', labels);

    expect(optionTexts(select).at(-1)).toBe('Monitor #2 · Disconnected');
    expect(selectedDisplay(select)).toBe('Monitor #2');
  });

  it('keeps the option elements when nothing changed so an open dropdown stays open', () => {
    const select = document.createElement('select');
    renderDisplaySelect(select, [builtIn, external], null, labels);
    const option = select.options[1];

    renderDisplaySelect(select, [builtIn, external], null, labels);

    expect(select.options[1]).toBe(option);
  });

  it('rebuilds the options when a display is connected', () => {
    const select = document.createElement('select');
    renderDisplaySelect(select, [builtIn], null, labels);

    renderDisplaySelect(select, [builtIn, external], null, labels);

    expect(select.options).toHaveLength(3);
  });

  it('finds the primary display from the name frames store', () => {
    expect(findDisplay([external, builtIn], 'primary')).toBe(builtIn);
    expect(findDisplay([external, builtIn], 'Monitor #2')).toBe(external);
    expect(findDisplay([external, builtIn], 'Monitor #9')).toBeUndefined();
  });

  it('falls back to the first display when the system reports no primary', () => {
    expect(findDisplay([external], 'primary')).toBe(external);
    expect(findDisplay([], 'primary')).toBeUndefined();
  });
});
