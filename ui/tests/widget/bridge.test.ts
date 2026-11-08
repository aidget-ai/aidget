import { describe, expect, it } from 'vite-plus/test';

import {
  errorMessage,
  failureOf,
  isServed,
  parseDiagnostic,
  parseDiagnosticDropped,
  parseDiagnosticReady,
  parsePointerEvent,
  parsePointerSubscription,
  parseReady,
  parseRequest,
  permissionsMessage,
  pointerMessage,
  resultMessage,
  routeRequest,
  HOST_SOURCE,
  SERVED_METHODS,
  WIDGET_SOURCE,
} from '../../src/widget/bridge';

const frameWindow = {} as Window;
const otherWindow = {} as Window;

describe('parseReady', () => {
  it('accepts a ready signal from the widget iframe', () => {
    expect(
      parseReady({ source: WIDGET_SOURCE, type: 'ready', revision: 7 }, frameWindow, frameWindow),
    ).toEqual({ revision: 7 });
  });

  it('rejects other windows, sources, types, and revisions', () => {
    const ready = { source: WIDGET_SOURCE, type: 'ready', revision: 7 };
    expect(parseReady(ready, otherWindow, frameWindow)).toBeNull();
    expect(parseReady(ready, null, frameWindow)).toBeNull();
    expect(parseReady({ ...ready, source: HOST_SOURCE }, frameWindow, frameWindow)).toBeNull();
    expect(parseReady({ ...ready, type: 'permissions' }, frameWindow, frameWindow)).toBeNull();
    expect(parseReady({ ...ready, revision: 0 }, frameWindow, frameWindow)).toBeNull();
    expect(parseReady({ ...ready, revision: '7' }, frameWindow, frameWindow)).toBeNull();
  });
});

describe('parseDiagnostic', () => {
  const diagnostic = {
    source: WIDGET_SOURCE,
    type: 'diagnostic',
    revision: 7,
    token: 'token',
    event: {
      source: 'error',
      revision: 7,
      message: 'boom',
      stack: 'at app.js:1:2',
      filename: 'app.js',
      line: 1,
      column: 2,
    },
  };

  it('accepts a bounded diagnostic from the current widget window', () => {
    expect(parseDiagnostic(diagnostic, frameWindow, frameWindow)).toEqual({
      revision: 7,
      token: 'token',
      event: diagnostic.event,
    });
  });

  it('rejects forged, malformed, oversized, and stale diagnostics', () => {
    expect(parseDiagnostic(diagnostic, otherWindow, frameWindow)).toBeNull();
    expect(
      parseDiagnostic(
        { ...diagnostic, event: { ...diagnostic.event, revision: 6 } },
        frameWindow,
        frameWindow,
      ),
    ).toBeNull();
    expect(
      parseDiagnostic(
        { ...diagnostic, event: { ...diagnostic.event, message: 'x'.repeat(8193) } },
        frameWindow,
        frameWindow,
      ),
    ).toBeNull();
    expect(
      parseDiagnostic(
        { ...diagnostic, event: { ...diagnostic.event, source: 'invented' } },
        frameWindow,
        frameWindow,
      ),
    ).toBeNull();
  });
});

describe('parseDiagnosticReady', () => {
  const ready = {
    source: WIDGET_SOURCE,
    type: 'diagnostic-ready',
    revision: 7,
    token: 'token',
  };

  it('accepts the injected collector handshake from the current iframe', () => {
    expect(parseDiagnosticReady(ready, frameWindow, frameWindow)).toEqual({
      revision: 7,
      token: 'token',
    });
  });

  it('rejects forged and incomplete handshakes', () => {
    expect(parseDiagnosticReady(ready, otherWindow, frameWindow)).toBeNull();
    expect(
      parseDiagnosticReady({ ...ready, source: HOST_SOURCE }, frameWindow, frameWindow),
    ).toBeNull();
    expect(parseDiagnosticReady({ ...ready, revision: 0 }, frameWindow, frameWindow)).toBeNull();
    expect(parseDiagnosticReady({ ...ready, token: '' }, frameWindow, frameWindow)).toBeNull();
  });
});

describe('parseDiagnosticDropped', () => {
  const dropped = {
    source: WIDGET_SOURCE,
    type: 'diagnostic-dropped',
    revision: 7,
    token: 'token',
    dropped: 12,
  };

  it('accepts an aggregated frontend drop count from the current iframe', () => {
    expect(parseDiagnosticDropped(dropped, frameWindow, frameWindow)).toEqual({
      revision: 7,
      token: 'token',
      dropped: 12,
    });
  });

  it('rejects forged, zero, and oversized drop counts', () => {
    expect(parseDiagnosticDropped(dropped, otherWindow, frameWindow)).toBeNull();
    expect(parseDiagnosticDropped({ ...dropped, dropped: 0 }, frameWindow, frameWindow)).toBeNull();
    expect(
      parseDiagnosticDropped({ ...dropped, dropped: 1_000_001 }, frameWindow, frameWindow),
    ).toBeNull();
  });
});

describe('pointer messages', () => {
  it('accepts a subscription from the widget iframe', () => {
    expect(
      parsePointerSubscription(
        {
          source: WIDGET_SOURCE,
          type: 'pointer-subscription',
          enabled: true,
          revision: 7,
          token: 'token',
        },
        frameWindow,
        frameWindow,
      ),
    ).toEqual({ enabled: true, revision: 7, token: 'token' });
  });

  it('rejects forged, incomplete, and stale subscriptions', () => {
    const subscription = {
      source: WIDGET_SOURCE,
      type: 'pointer-subscription',
      enabled: true,
      revision: 7,
      token: 'token',
    };
    expect(parsePointerSubscription(subscription, otherWindow, frameWindow)).toBeNull();
    expect(
      parsePointerSubscription({ ...subscription, source: HOST_SOURCE }, frameWindow, frameWindow),
    ).toBeNull();
    expect(
      parsePointerSubscription({ ...subscription, enabled: 'yes' }, frameWindow, frameWindow),
    ).toBeNull();
    expect(
      parsePointerSubscription({ ...subscription, revision: 0 }, frameWindow, frameWindow),
    ).toBeNull();
    expect(
      parsePointerSubscription({ ...subscription, token: '' }, frameWindow, frameWindow),
    ).toBeNull();
  });

  it('validates host events and wraps them for the iframe', () => {
    expect(parsePointerEvent({ type: 'leave', x: 10, y: 20 })).toEqual({
      type: 'leave',
      x: 10,
      y: 20,
    });
    expect(parsePointerEvent({ type: 'move', x: Number.NaN, y: 20 })).toBeNull();
    expect(parsePointerEvent({ type: 'down', x: 10, y: 20 })).toBeNull();
    expect(pointerMessage({ type: 'move', x: 10, y: 20 }, 7, 'token')).toEqual({
      source: HOST_SOURCE,
      type: 'pointer',
      revision: 7,
      token: 'token',
      event: { type: 'move', x: 10, y: 20 },
    });
  });
});

describe('parseRequest', () => {
  it('accepts a request from the widget iframe', () => {
    const request = parseRequest(
      { source: WIDGET_SOURCE, id: 'aidget-1', method: 'location' },
      frameWindow,
      frameWindow,
    );
    expect(request).toEqual({
      source: WIDGET_SOURCE,
      id: 'aidget-1',
      method: 'location',
      params: undefined,
    });
  });

  it('ignores messages from any other window', () => {
    const data = { source: WIDGET_SOURCE, id: 'aidget-2', method: 'location' };
    expect(parseRequest(data, otherWindow, frameWindow)).toBeNull();
    expect(parseRequest(data, null, frameWindow)).toBeNull();
  });

  it('ignores messages without an id or method', () => {
    expect(
      parseRequest({ source: WIDGET_SOURCE, method: 'location' }, frameWindow, frameWindow),
    ).toBeNull();
    expect(parseRequest({ source: WIDGET_SOURCE, id: 'x' }, frameWindow, frameWindow)).toBeNull();
    expect(parseRequest('nope', frameWindow, frameWindow)).toBeNull();
  });

  it('only accepts the widget source tag', () => {
    const forged = { source: HOST_SOURCE, id: 'aidget-3', method: 'location' };
    expect(parseRequest(forged, frameWindow, frameWindow)).toBeNull();
  });
});

describe('failureOf', () => {
  it('keeps the code the host attached', () => {
    expect(failureOf({ code: 'denied', message: 'location denied' })).toEqual({
      code: 'denied',
      message: 'location denied',
    });
  });

  it('falls back to unavailable for an opaque rejection', () => {
    expect(failureOf('boom')).toEqual({ code: 'unavailable', message: 'boom' });
    expect(failureOf(new Error('x'))).toEqual({ code: 'unavailable', message: 'Error: x' });
  });
});

describe('host messages', () => {
  it('tags permission updates for the widget', () => {
    expect(permissionsMessage(['location'])).toEqual({
      source: HOST_SOURCE,
      type: 'permissions',
      permissions: ['location'],
    });
  });

  it('answers a request by id', () => {
    expect(resultMessage('aidget-4', { latitude: 1 })).toEqual({
      source: HOST_SOURCE,
      id: 'aidget-4',
      ok: true,
      result: { latitude: 1 },
    });
    expect(errorMessage('aidget-5', { code: 'timeout', message: 'timeout' })).toEqual({
      source: HOST_SOURCE,
      id: 'aidget-5',
      ok: false,
      error: { code: 'timeout', message: 'timeout' },
    });
  });
});

describe('isServed', () => {
  it('only serves the methods the host implements', () => {
    expect(SERVED_METHODS).toContain('location');
    expect(SERVED_METHODS).toContain('fetch');
    expect(SERVED_METHODS).toContain('open_url');
    expect(SERVED_METHODS).toContain('storage_get');
    expect(SERVED_METHODS).toContain('storage_set');
    expect(SERVED_METHODS).toContain('storage_remove');
    expect(SERVED_METHODS).toContain('storage_clear');
    expect(isServed('location')).toBe(true);
    expect(isServed('camera')).toBe(false);
  });
});

describe('routeRequest', () => {
  it('maps location and storage calls to Tauri commands', () => {
    expect(routeRequest('location', undefined)).toEqual({
      command: 'widget_location',
      args: {},
    });
    expect(routeRequest('fetch', { url: 'https://api.example.com/items' })).toEqual({
      command: 'widget_fetch',
      args: {
        url: 'https://api.example.com/items',
        method: 'GET',
        headers: {},
        body: null,
      },
    });
    expect(
      routeRequest('fetch', {
        url: 'https://api.example.com/items',
        options: {
          method: 'POST',
          headers: { Accept: 'application/vnd.example+json', 'X-Token': 'secret' },
          body: '{"ok":true}',
        },
      }),
    ).toEqual({
      command: 'widget_fetch',
      args: {
        url: 'https://api.example.com/items',
        method: 'POST',
        headers: { Accept: 'application/vnd.example+json', 'X-Token': 'secret' },
        body: '{"ok":true}',
      },
    });
    expect(routeRequest('open_url', { url: 'https://www.example.com/story' })).toEqual({
      command: 'widget_open_url',
      args: { url: 'https://www.example.com/story' },
    });
    expect(routeRequest('storage_get', { key: 'todo' })).toEqual({
      command: 'widget_storage_get',
      args: { key: 'todo' },
    });
    expect(routeRequest('storage_set', { key: 'todo', value: 'done' })).toEqual({
      command: 'widget_storage_set',
      args: { key: 'todo', value: 'done' },
    });
    expect(routeRequest('storage_remove', { key: 'todo' })).toEqual({
      command: 'widget_storage_remove',
      args: { key: 'todo' },
    });
    expect(routeRequest('storage_clear', undefined)).toEqual({
      command: 'widget_storage_clear',
      args: {},
    });
  });

  it('rejects invalid storage parameters before invoking the host', () => {
    expect(routeRequest('storage_get', { key: '' })).toEqual({
      error: {
        code: 'invalid_params',
        message: 'storage key must be a non-empty string',
      },
    });
    expect(routeRequest('storage_set', { key: 'todo', value: 1 })).toEqual({
      error: {
        code: 'invalid_params',
        message: 'storage value must be a string',
      },
    });
    expect(routeRequest('fetch', { url: '' })).toEqual({
      error: {
        code: 'invalid_params',
        message: 'url must be a non-empty string',
      },
    });
    expect(
      routeRequest('fetch', {
        url: 'https://api.example.com/items',
        options: { method: 1 },
      }),
    ).toEqual({
      error: {
        code: 'invalid_params',
        message: 'fetch method must be a non-empty string',
      },
    });
    expect(
      routeRequest('fetch', {
        url: 'https://api.example.com/items',
        options: { headers: { Authorization: 1 } },
      }),
    ).toEqual({
      error: {
        code: 'invalid_params',
        message: 'fetch headers must map strings to strings',
      },
    });
    expect(
      routeRequest('fetch', {
        url: 'https://api.example.com/items',
        options: { body: 1 },
      }),
    ).toEqual({
      error: {
        code: 'invalid_params',
        message: 'fetch body must be a string or null',
      },
    });
    expect(routeRequest('camera', undefined)).toBeNull();
  });
});
