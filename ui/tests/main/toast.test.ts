// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vite-plus/test';

import { createToastRegion } from '../../src/main/toast';

beforeEach(() => {
  vi.useFakeTimers();
  document.body.replaceChildren();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('main window toast', () => {
  it('announces a message and dismisses it automatically', () => {
    const toast = createToastRegion(document.body);

    toast.show('Installed in Local Widgets');

    const region = document.querySelector<HTMLElement>('.toast-region')!;
    expect(region.getAttribute('role')).toBe('status');
    expect(region.getAttribute('aria-live')).toBe('polite');
    expect(region.textContent).toBe('Installed in Local Widgets');
    expect(document.querySelector('.toast svg')?.getAttribute('aria-hidden')).toBe('true');

    vi.advanceTimersByTime(4499);
    expect(region.textContent).toBe('Installed in Local Widgets');

    vi.advanceTimersByTime(1);

    expect(region.textContent).toBe('');
  });

  it('replaces the active message when another action completes', () => {
    const toast = createToastRegion(document.body);

    toast.show('First');
    toast.show('Second');

    expect(document.querySelector('.toast')?.textContent).toBe('Second');
  });
});
