/// @vitest-environment happy-dom

import { readFileSync } from 'node:fs';

import { describe, expect, it, vi } from 'vite-plus/test';

const widgetHtml = readFileSync('ui/widget.html', 'utf8');
const shellHtml = widgetHtml
  .replace('    <link rel="stylesheet" href="./src/widget/index.css" />\n', '')
  .replace(
    '      <iframe class="widget" title="aidget widget" sandbox referrerpolicy="no-referrer"></iframe>\n',
    '',
  )
  .replace('    <script type="module" src="./src/widget/index.ts"></script>\n', '');

describe('widget window shell', () => {
  it('hides fallback surfaces and faults before widget state loads', () => {
    const page = new DOMParser().parseFromString(shellHtml, 'text/html');
    const chrome = page.querySelector<HTMLElement>('.widget-chrome');
    const surface = page.querySelector<HTMLElement>('.host-surface');
    const fault = page.querySelector<HTMLElement>('.fault');

    expect(chrome?.dataset.state).toBe('loading');
    expect(surface?.hidden).toBe(true);
    expect(fault?.hidden).toBe(true);
  });
});

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ label: 'widget-grok', onMoved: vi.fn(async () => () => {}) }),
}));

it('wires authorized animation requests and cancels them when content or permissions change', async () => {
  const { invoke } = await import('@tauri-apps/api/core');
  const state = {
    id: 'grok',
    revision: 2,
    token: 'current',
    permissions: ['script'],
    dragMode: 'all',
    hostSurface: false,
    title: 'Grok',
  };
  vi.mocked(invoke).mockResolvedValue(state);
  document.body.innerHTML = new DOMParser().parseFromString(shellHtml, 'text/html').body.innerHTML;
  // Keep the iframe detached: this test covers shell wiring without navigating happy-dom.
  const frame = document.createElement('iframe');
  const post = vi.fn();
  const child = { postMessage: post } as unknown as Window;
  vi.spyOn(frame, 'contentWindow', 'get').mockReturnValue(child);
  const query = document.querySelector.bind(document);
  const querySpy = vi
    .spyOn(document, 'querySelector')
    .mockImplementation((selector) => (selector === 'iframe.widget' ? frame : query(selector)));
  const scheduled = new Map<number, FrameRequestCallback>();
  let next = 0;
  const raf = vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
    scheduled.set(++next, callback);
    return next;
  });
  const cancel = vi.spyOn(window, 'cancelAnimationFrame').mockImplementation((handle) => {
    scheduled.delete(handle);
  });
  try {
    await import('../../src/widget/index');
    await Promise.resolve();
    const request = {
      source: 'aidget-widget',
      type: 'animation-frame-request',
      revision: 2,
      token: 'current',
      session: 'document',
      request: 1,
    };
    const send = (data: unknown) =>
      window.dispatchEvent(new MessageEvent('message', { source: frame.contentWindow, data }));
    send(request);
    expect(scheduled.size).toBe(1);
    const [handle, callback] = Array.from(scheduled)[0];
    scheduled.delete(handle);
    callback(16);
    expect(post).toHaveBeenCalledWith(
      expect.objectContaining({
        source: 'aidget-host',
        type: 'animation-frame',
        session: 'document',
        request: 1,
      }),
      '*',
    );
    send({ ...request, request: 2 });
    expect(scheduled.size).toBe(1);
    const apply = (window as unknown as { __aidgetApply: (state: unknown) => void }).__aidgetApply;
    apply({ ...state, revision: 3, token: 'updated', permissions: [] });
    expect(scheduled.size).toBe(0);
    send({ ...request, revision: 3, token: 'updated', request: 3 });
    expect(scheduled.size).toBe(0);
    send(request);
    expect(scheduled.size).toBe(0);
  } finally {
    raf.mockRestore();
    cancel.mockRestore();
    querySpy.mockRestore();
  }
});
