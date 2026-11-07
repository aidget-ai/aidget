import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

import { describe, expect, it, vi } from 'vite-plus/test';

const SOURCE = readFileSync(new URL('../../src/widget/api.js', import.meta.url), 'utf8');

interface Harness {
  api: {
    permissions: string[];
    ready: () => void;
    requestLocation: () => Promise<unknown>;
    fetch: (url: unknown, options?: unknown) => Promise<unknown>;
    openUrl: (url: unknown) => Promise<unknown>;
    onPointer: (handler: (event: { type: string; x: number; y: number }) => void) => () => void;
    storage: {
      getItem: (key: unknown) => Promise<unknown>;
      setItem: (key: unknown, value: unknown) => Promise<unknown>;
      removeItem: (key: unknown) => Promise<unknown>;
      clear: () => Promise<unknown>;
    };
  } | null;
  posted: Array<Record<string, unknown>>;
  errors: unknown[];
  send: (data: unknown, source?: unknown) => void;
  emitWindow: (name: string, event?: unknown) => void;
}

/** Run the injected script the way the host does: before the page's scripts. */
function install(
  pathname = '/content/weather/index.html',
  hostname = 'localhost',
  search = '?v=2-token',
): Harness {
  const listeners = new Map<string, Array<(event: unknown) => void>>();
  const posted: Array<Record<string, unknown>> = [];
  const errors: unknown[] = [];
  const parent = {
    postMessage: (data: Record<string, unknown>) => posted.push(data),
  };

  const sandbox: Record<string, unknown> = {
    location: { pathname, hostname, protocol: 'aidget:', search },
    setTimeout,
    clearTimeout,
    Map,
    console: { error: (error: unknown) => errors.push(error) },
  };
  sandbox.window = {
    parent,
    addEventListener: (name: string, handler: (event: unknown) => void) => {
      const handlers = listeners.get(name) ?? [];
      handlers.push(handler);
      listeners.set(name, handlers);
    },
  };

  runInNewContext(SOURCE, sandbox);

  return {
    api: (sandbox.window as { aidet?: never; aidget?: Harness['api'] }).aidget ?? null,
    posted,
    errors,
    send: (data, source: unknown = parent) => {
      for (const listener of listeners.get('message') ?? []) {
        listener({ source, data });
      }
    },
    emitWindow: (name, event) => {
      for (const listener of listeners.get(name) ?? []) {
        listener(event);
      }
    },
  };
}

describe('widget api installation', () => {
  it('stays out of the shell page and the main-window preview', () => {
    expect(install('/widget').api).toBeNull();
    expect(install('/main').api).toBeNull();
  });

  it('installs inside a widget document', () => {
    const harness = install();
    expect(harness.api?.permissions).toEqual([]);
    expect(typeof harness.api?.ready).toBe('function');
    expect(typeof harness.api?.requestLocation).toBe('function');
    expect(typeof harness.api?.fetch).toBe('function');
    expect(typeof harness.api?.openUrl).toBe('function');
    expect(typeof harness.api?.onPointer).toBe('function');
    expect(typeof harness.api?.storage.getItem).toBe('function');
  });

  it('installs under the Windows http host of the same protocol', () => {
    const harness = install('/content/weather/index.html', 'aidget.localhost');
    expect(typeof harness.api?.requestLocation).toBe('function');
  });

  it('stays out of a page that is not served by aidget', () => {
    expect(install('/content/weather/index.html', 'example.com').api).toBeNull();
  });
});

describe('ready', () => {
  it('announces the current revision once', () => {
    const harness = install('/content/weather/index.html', 'localhost', '?v=12-token');

    harness.api!.ready();
    harness.api!.ready();

    expect(harness.posted).toEqual([{ source: 'aidget-widget', type: 'ready', revision: 12 }]);
  });
});

describe('requestLocation', () => {
  it('posts a request tagged for the host and resolves with the reply', async () => {
    const harness = install();
    const pending = harness.api!.requestLocation();

    expect(harness.posted).toHaveLength(1);
    const [request] = harness.posted;
    expect(request).toMatchObject({ source: 'aidget-widget', method: 'location' });

    harness.send({
      source: 'aidget-host',
      id: request.id,
      ok: true,
      result: { latitude: 30.64, longitude: 104.04, accuracy: 65, place: '武侯区' },
    });

    await expect(pending).resolves.toEqual({
      latitude: 30.64,
      longitude: 104.04,
      accuracy: 65,
      place: '武侯区',
    });
  });

  it('rejects with the host error code', async () => {
    const harness = install();
    const pending = harness.api!.requestLocation();
    const [request] = harness.posted;
    harness.send({
      source: 'aidget-host',
      id: request.id,
      ok: false,
      error: { code: 'denied', message: 'the user refused' },
    });

    await expect(pending).rejects.toMatchObject({
      code: 'denied',
      message: 'the user refused',
    });
  });

  it('ignores replies that do not come from the host frame', async () => {
    const harness = install();
    const pending = harness.api!.requestLocation();
    const [request] = harness.posted;
    harness.send({ source: 'aidget-host', id: request.id, ok: true, result: { latitude: 0 } }, {});

    let settled = false;
    void pending.then(
      () => (settled = true),
      () => (settled = true),
    );
    await Promise.resolve();
    expect(settled).toBe(false);
  });

  it('times out when the host never answers', async () => {
    vi.useFakeTimers();
    try {
      const harness = install();
      const pending = harness.api!.requestLocation();
      vi.advanceTimersByTime(25_000);
      await expect(pending).rejects.toMatchObject({ code: 'timeout' });
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('permissions', () => {
  it('mirrors the declaration the shell pushes', () => {
    const harness = install();
    harness.send({ source: 'aidget-host', type: 'permissions', permissions: ['location'] });
    expect(harness.api?.permissions).toEqual(['location']);

    harness.send({ source: 'aidget-host', type: 'permissions' });
    expect(harness.api?.permissions).toEqual([]);
  });
});

describe('pointer hover', () => {
  it('subscribes once, fans out local events, and unsubscribes idempotently', () => {
    const harness = install();
    const first = vi.fn();
    const second = vi.fn();
    const stopFirst = harness.api!.onPointer(first);
    const stopSecond = harness.api!.onPointer(second);

    expect(harness.posted).toEqual([
      {
        source: 'aidget-widget',
        type: 'pointer-subscription',
        enabled: true,
        revision: 2,
        token: 'token',
      },
    ]);

    harness.send({
      source: 'aidget-host',
      type: 'pointer',
      revision: 2,
      token: 'token',
      event: { type: 'move', x: 12.5, y: 8 },
    });
    expect(first).toHaveBeenCalledWith({ type: 'move', x: 12.5, y: 8 });
    expect(second).toHaveBeenCalledWith({ type: 'move', x: 12.5, y: 8 });

    stopFirst();
    stopFirst();
    expect(harness.posted).toHaveLength(1);

    stopSecond();
    expect(harness.posted[1]).toEqual({
      source: 'aidget-widget',
      type: 'pointer-subscription',
      enabled: false,
      revision: 2,
      token: 'token',
    });
  });

  it('rejects non-functions and isolates handler failures', () => {
    const harness = install();
    expect(() => harness.api!.onPointer(undefined as never)).toThrow(
      'onPointer requires a function',
    );

    const failure = new Error('boom');
    const second = vi.fn();
    harness.api!.onPointer(() => {
      throw failure;
    });
    harness.api!.onPointer(second);
    harness.send({
      source: 'aidget-host',
      type: 'pointer',
      revision: 2,
      token: 'token',
      event: { type: 'enter', x: 0, y: 0 },
    });

    expect(harness.errors).toEqual([failure]);
    expect(second).toHaveBeenCalledOnce();
  });

  it('ignores stale revisions, stale tokens, and malformed events', () => {
    const harness = install();
    const handler = vi.fn();
    harness.api!.onPointer(handler);

    for (const event of [
      {
        source: 'aidget-host',
        type: 'pointer',
        revision: 1,
        token: 'token',
        event: { type: 'move', x: 1, y: 2 },
      },
      {
        source: 'aidget-host',
        type: 'pointer',
        revision: 2,
        token: 'other',
        event: { type: 'move', x: 1, y: 2 },
      },
      {
        source: 'aidget-host',
        type: 'pointer',
        revision: 2,
        token: 'token',
        event: { type: 'move', x: Number.NaN, y: 2 },
      },
    ]) {
      harness.send(event);
    }

    expect(handler).not.toHaveBeenCalled();
  });

  it('disables an active subscription when the document is hidden', () => {
    const harness = install();
    harness.api!.onPointer(vi.fn());
    harness.emitWindow('pagehide');

    expect(harness.posted.at(-1)).toEqual({
      source: 'aidget-widget',
      type: 'pointer-subscription',
      enabled: false,
      revision: 2,
      token: 'token',
    });
  });
});

describe('storage', () => {
  it('posts storage calls with their parameters', async () => {
    const harness = install();

    const get = harness.api!.storage.getItem('todos');
    expect(harness.posted[0]).toMatchObject({
      source: 'aidget-widget',
      method: 'storage_get',
      params: { key: 'todos' },
    });
    harness.send({
      source: 'aidget-host',
      id: harness.posted[0].id,
      ok: true,
      result: '{"first":true}',
    });
    await expect(get).resolves.toBe('{"first":true}');

    const set = harness.api!.storage.setItem('todos', '{}');
    expect(harness.posted[1]).toMatchObject({
      source: 'aidget-widget',
      method: 'storage_set',
      params: { key: 'todos', value: '{}' },
    });
    harness.send({
      source: 'aidget-host',
      id: harness.posted[1].id,
      ok: true,
      result: null,
    });
    await expect(set).resolves.toBeNull();
  });

  it('supports remove, clear, and host failures', async () => {
    const harness = install();

    const remove = harness.api!.storage.removeItem('todos');
    const clear = harness.api!.storage.clear();
    expect(harness.posted[0]).toMatchObject({
      method: 'storage_remove',
      params: { key: 'todos' },
    });
    expect(harness.posted[1]).toMatchObject({
      method: 'storage_clear',
      params: undefined,
    });
    harness.send({
      source: 'aidget-host',
      id: harness.posted[0].id,
      ok: false,
      error: { code: 'permission_denied', message: 'storage not declared' },
    });
    harness.send({
      source: 'aidget-host',
      id: harness.posted[1].id,
      ok: true,
      result: null,
    });

    await expect(remove).rejects.toMatchObject({
      code: 'permission_denied',
      message: 'storage not declared',
    });
    await expect(clear).resolves.toBeNull();
  });
});

describe('network and links', () => {
  it('posts JSON and external-link requests with their URL', async () => {
    const harness = install();

    const json = harness.api!.fetch('https://api.example.com/items');
    expect(harness.posted[0]).toMatchObject({
      source: 'aidget-widget',
      method: 'fetch',
      params: { url: 'https://api.example.com/items', options: null },
    });
    harness.send({
      source: 'aidget-host',
      id: harness.posted[0].id,
      ok: true,
      result: { items: [] },
    });
    await expect(json).resolves.toEqual({ items: [] });

    const opened = harness.api!.openUrl('https://www.example.com/story');
    expect(harness.posted[1]).toMatchObject({
      source: 'aidget-widget',
      method: 'open_url',
      params: { url: 'https://www.example.com/story' },
    });
    harness.send({
      source: 'aidget-host',
      id: harness.posted[1].id,
      ok: true,
      result: null,
    });
    await expect(opened).resolves.toBeNull();
  });

  it('forwards fetch method, headers and body', async () => {
    const harness = install();

    const pending = harness.api!.fetch('https://api.example.com/items', {
      method: 'POST',
      headers: { Accept: 'application/vnd.example+json', 'X-Token': 'secret' },
      body: '{"ok":true}',
    });

    expect(harness.posted[0]).toMatchObject({
      source: 'aidget-widget',
      method: 'fetch',
      params: {
        url: 'https://api.example.com/items',
        options: {
          method: 'POST',
          headers: { Accept: 'application/vnd.example+json', 'X-Token': 'secret' },
          body: '{"ok":true}',
        },
      },
    });
    harness.send({
      source: 'aidget-host',
      id: harness.posted[0].id,
      ok: true,
      result: { ok: true },
    });
    await expect(pending).resolves.toEqual({ ok: true });
  });

  it('surfaces host network failures', async () => {
    const harness = install();
    const pending = harness.api!.fetch('https://api.example.com/items');
    harness.send({
      source: 'aidget-host',
      id: harness.posted[0].id,
      ok: false,
      error: { code: 'http_error', message: 'HTTP 403' },
    });

    await expect(pending).rejects.toMatchObject({
      code: 'http_error',
      message: 'HTTP 403',
    });
  });
});
