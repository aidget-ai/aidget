import { describe, expect, it, vi } from 'vite-plus/test';

import {
  createAnimationHost,
  parseAnimationRequest,
  type AnimationRequest,
} from '../../src/widget/animation-host';
import type { WidgetState } from '../../src/widget/url';

const frame = {} as Window;
const state: WidgetState = {
  id: 'grok',
  revision: 2,
  token: 'current',
  dragMode: 'all',
  permissions: ['script'],
  hostSurface: false,
  title: 'Grok',
};
const request: AnimationRequest = {
  type: 'animation-frame-request',
  revision: 2,
  token: 'current',
  session: 'document-1',
  request: 1,
};

function setup() {
  let next = 0;
  const callbacks = new Map<number, FrameRequestCallback>();
  const post = vi.fn();
  const clock = {
    requestAnimationFrame: vi.fn((callback: FrameRequestCallback) => {
      callbacks.set(++next, callback);
      return next;
    }),
    cancelAnimationFrame: vi.fn((id: number) => {
      callbacks.delete(id);
    }),
    performance: { timeOrigin: 1000 },
  };
  const host = createAnimationHost(post, clock);
  function tick(timestamp: number) {
    const batch = Array.from(callbacks.values());
    callbacks.clear();
    for (const callback of batch) {
      callback(timestamp);
    }
  }
  return { host, post, clock, callbacks, tick };
}

describe('animation frame authorization', () => {
  it('accepts only the current owned document with the script grant', () => {
    const data = { ...request, source: 'aidget-widget' };
    expect(parseAnimationRequest({ source: frame, data }, frame, state)).toEqual(request);
    for (const patch of [
      { source: 'aidget-host' },
      { revision: 1 },
      { token: 'old' },
      { request: 0 },
      { request: NaN },
      { session: '' },
      { session: 'a'.repeat(129) },
      { type: 'pointer' },
    ]) {
      expect(
        parseAnimationRequest({ source: frame, data: { ...data, ...patch } }, frame, state),
      ).toBeNull();
    }
    expect(parseAnimationRequest({ source: {} as Window, data }, frame, state)).toBeNull();
    expect(
      parseAnimationRequest({ source: frame, data }, frame, { ...state, permissions: [] }),
    ).toBeNull();
    expect(
      parseAnimationRequest(
        { source: frame, data: { ...data, type: 'animation-frame-cancel' } },
        frame,
        { ...state, permissions: [] },
      ),
    ).toMatchObject({ type: 'animation-frame-cancel' });
  });
});

describe('host animation clock', () => {
  it('delivers one frame in absolute time and stays idle until another request', () => {
    const { host, post, callbacks, tick } = setup();
    expect(callbacks.size).toBe(0);
    host.accept(request);
    tick(16);
    expect(post).toHaveBeenCalledWith({
      ...request,
      source: 'aidget-host',
      type: 'animation-frame',
      timestamp: 1016,
    });
    tick(32);
    expect(post).toHaveBeenCalledOnce();
    expect(callbacks.size).toBe(0);
  });

  it('retains at most one pending frame and cancels only the matching batch', () => {
    const { host, callbacks, post, tick } = setup();
    host.accept(request);
    host.accept({ ...request, request: 2 });
    host.accept({ ...request, type: 'animation-frame-cancel' });
    expect(callbacks.size).toBe(1);
    tick(32);
    expect(post).toHaveBeenCalledWith(expect.objectContaining({ request: 2 }));
    host.accept({ ...request, request: 3 });
    host.accept({ ...request, request: 3, type: 'animation-frame-cancel' });
    expect(callbacks.size).toBe(0);
  });

  it('separates same-revision document reloads', () => {
    const { host, callbacks, post, tick } = setup();
    host.accept(request);
    host.accept({ ...request, session: 'document-2' });
    host.accept({ ...request, type: 'animation-frame-cancel' });
    expect(callbacks.size).toBe(1);
    tick(32);
    expect(post).toHaveBeenCalledWith(expect.objectContaining({ session: 'document-2' }));
  });

  it('pauses pending requests while hidden and releases them on navigation', () => {
    const { host, callbacks, post, tick } = setup();
    host.accept(request);
    host.setVisible(false);
    expect(callbacks.size).toBe(0);
    host.setVisible(true);
    expect(callbacks.size).toBe(1);
    tick(1000);
    expect(post).toHaveBeenCalledOnce();
    host.accept({ ...request, request: 2 });
    host.reset();
    tick(1016);
    expect(post).toHaveBeenCalledOnce();
    host.setVisible(false);
    host.accept({ ...request, request: 3 });
    host.reset();
    host.setVisible(true);
    expect(callbacks.size).toBe(0);
  });
});
