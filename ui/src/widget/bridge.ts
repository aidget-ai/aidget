/**
 * Message contract between the widget shell and the sandboxed widget document.
 *
 * The iframe is sandboxed without `allow-same-origin`, so its origin is opaque
 * and `postMessage` must use `*`. Identity comes from the frame reference the
 * shell already holds, never from the payload.
 */

export const WIDGET_SOURCE = 'aidget-widget';
export const HOST_SOURCE = 'aidget-host';

export interface BridgeRequest {
  source: typeof WIDGET_SOURCE;
  id: string;
  method: string;
  params?: Record<string, unknown>;
}

export interface BridgeFailure {
  code: string;
  message: string;
}

export interface ReadySignal {
  revision: number;
}

/** Parse the optional one-way readiness signal sent by a widget document. */
export function parseReady(
  data: unknown,
  source: MessageEventSource | null,
  frameWindow: Window | null,
): ReadySignal | null {
  if (!frameWindow || source !== frameWindow) {
    return null;
  }
  if (!data || typeof data !== 'object') {
    return null;
  }
  const candidate = data as { source?: unknown; type?: unknown; revision?: unknown };
  if (candidate.source !== WIDGET_SOURCE || candidate.type !== 'ready') {
    return null;
  }
  if (
    typeof candidate.revision !== 'number' ||
    !Number.isSafeInteger(candidate.revision) ||
    candidate.revision <= 0
  ) {
    return null;
  }
  return { revision: candidate.revision };
}

export function parseRequest(
  data: unknown,
  source: MessageEventSource | null,
  frameWindow: Window | null,
): BridgeRequest | null {
  if (!frameWindow || source !== frameWindow) {
    return null;
  }
  if (!data || typeof data !== 'object') {
    return null;
  }
  const candidate = data as Partial<BridgeRequest>;
  if (candidate.source !== WIDGET_SOURCE) {
    return null;
  }
  if (typeof candidate.id !== 'string' || !candidate.id) {
    return null;
  }
  if (typeof candidate.method !== 'string' || !candidate.method) {
    return null;
  }
  return {
    source: WIDGET_SOURCE,
    id: candidate.id,
    method: candidate.method,
    params: candidate.params,
  };
}

export function failureOf(error: unknown): BridgeFailure {
  if (error && typeof error === 'object') {
    const shape = error as { code?: unknown; message?: unknown };
    if (typeof shape.code === 'string' && shape.code) {
      return {
        code: shape.code,
        message: typeof shape.message === 'string' ? shape.message : shape.code,
      };
    }
  }
  return { code: 'unavailable', message: String(error) };
}

export function permissionsMessage(permissions: string[]) {
  return { source: HOST_SOURCE, type: 'permissions', permissions };
}

export function resultMessage(id: string, result: unknown) {
  return { source: HOST_SOURCE, id, ok: true, result };
}

export function errorMessage(id: string, error: BridgeFailure) {
  return { source: HOST_SOURCE, id, ok: false, error };
}

/** Methods the host will serve; anything else is refused without an IPC call. */
export const SERVED_METHODS = [
  'location',
  'fetch',
  'open_url',
  'storage_get',
  'storage_set',
  'storage_remove',
  'storage_clear',
] as const;

export function isServed(method: string): boolean {
  return (SERVED_METHODS as readonly string[]).includes(method);
}

export type BridgeRoute =
  | {
      command: string;
      args: Record<string, unknown>;
    }
  | {
      error: BridgeFailure;
    };

/** Translate a validated widget request into the one Tauri command it may call. */
export function routeRequest(
  method: string,
  params: Record<string, unknown> | undefined,
): BridgeRoute | null {
  switch (method) {
    case 'location':
      return { command: 'widget_location', args: {} };
    case 'fetch': {
      const url = stringParam(params, 'url');
      if (!url) {
        return invalidUrl();
      }
      const options = params?.options;
      if (
        options !== undefined &&
        options !== null &&
        (typeof options !== 'object' || Array.isArray(options))
      ) {
        return invalidFetch('fetch options must be an object or null');
      }
      const candidate = (options ?? {}) as Record<string, unknown>;
      const method = candidate.method;
      if (method !== undefined && (typeof method !== 'string' || !method.trim())) {
        return invalidFetch('fetch method must be a non-empty string');
      }
      const rawHeaders = candidate.headers;
      if (
        rawHeaders !== undefined &&
        rawHeaders !== null &&
        (typeof rawHeaders !== 'object' ||
          Array.isArray(rawHeaders) ||
          Object.values(rawHeaders).some((value) => typeof value !== 'string'))
      ) {
        return invalidFetch('fetch headers must map strings to strings');
      }
      const body = candidate.body;
      if (body !== undefined && body !== null && typeof body !== 'string') {
        return invalidFetch('fetch body must be a string or null');
      }
      const headers = (rawHeaders ?? {}) as Record<string, string>;
      return {
        command: 'widget_fetch',
        args: {
          url,
          method: typeof method === 'string' ? method : 'GET',
          headers,
          body: typeof body === 'string' ? body : null,
        },
      };
    }
    case 'open_url': {
      const url = stringParam(params, 'url');
      if (!url) {
        return invalidUrl();
      }
      return { command: 'widget_open_url', args: { url } };
    }
    case 'storage_get':
    case 'storage_remove': {
      const key = stringParam(params, 'key');
      if (!key) {
        return invalidKey();
      }
      return { command: `widget_${method}`, args: { key } };
    }
    case 'storage_set': {
      const key = stringParam(params, 'key');
      const value = stringParam(params, 'value');
      if (!key) {
        return invalidKey();
      }
      if (value === null) {
        return {
          error: {
            code: 'invalid_params',
            message: 'storage value must be a string',
          },
        };
      }
      return { command: 'widget_storage_set', args: { key, value } };
    }
    case 'storage_clear':
      return { command: 'widget_storage_clear', args: {} };
    default:
      return null;
  }
}

function stringParam(params: Record<string, unknown> | undefined, name: string): string | null {
  const value = params?.[name];
  return typeof value === 'string' ? value : null;
}

function invalidKey(): BridgeRoute {
  return {
    error: {
      code: 'invalid_params',
      message: 'storage key must be a non-empty string',
    },
  };
}

function invalidUrl(): BridgeRoute {
  return {
    error: {
      code: 'invalid_params',
      message: 'url must be a non-empty string',
    },
  };
}

function invalidFetch(message: string): BridgeRoute {
  return {
    error: {
      code: 'invalid_params',
      message,
    },
  };
}
