import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

import {
  errorMessage,
  failureOf,
  parsePointerEvent,
  parsePointerSubscription,
  parseReady,
  parseRequest,
  permissionsMessage,
  pointerMessage,
  resultMessage,
  routeRequest,
} from './bridge';
import { createDragSession, parseDragSignal, type DragSignal } from './drag-host';
import {
  contentOrigin,
  widgetUrl,
  widgetFramePolicy,
  widgetEventTarget,
  widgetIdFromLabel,
  isOwnWidget,
  sandboxAttribute,
  shouldReload,
  type WidgetState,
} from './url';

const chrome = document.querySelector<HTMLElement>('.widget-chrome')!;
const frame = document.querySelector<HTMLIFrameElement>('iframe.widget')!;
const hostSurface = document.querySelector<HTMLElement>('.host-surface')!;
const fault = document.querySelector<HTMLElement>('.fault')!;
const faultMessage = document.querySelector<HTMLElement>('[data-fault-message]')!;
const handle = document.querySelector<HTMLElement>('[data-drag-handle]')!;

let state: WidgetState | null = null;
let pendingRevision = 0;
let pointerEnabled = false;
let pointerToggle = Promise.resolve();
const appWindow = getCurrentWindow();
const ownId = widgetIdFromLabel(appWindow.label);
const origin = contentOrigin(window.location);
if (ownId === null) {
  throw new Error('not a widget window');
}
// A static document must not navigate itself to an unprotected SVG or external page.
const framePolicy = document.createElement('meta');
framePolicy.httpEquiv = 'Content-Security-Policy';
framePolicy.content = widgetFramePolicy(origin, ownId);
document.head.appendChild(framePolicy);
const dragSession = createDragSession(
  () => invoke<void>('begin_widget_drag'),
  () => invoke<void>('start_widget_drag'),
  () => invoke<void>('end_widget_drag'),
);
let activeDrag: DragSignal | null = null;

function apply(next: WidgetState) {
  // The label, not the first event, decides which widget this window shows.
  // A leaked or early event must never swap the document to another widget.
  if (!isOwnWidget(ownId, next)) {
    return;
  }
  const reload = shouldReload(state, next, frame.src, origin);
  const sandboxChanged =
    !state || state.permissions.includes('script') !== next.permissions.includes('script');
  state = next;

  frame.setAttribute(
    'sandbox',
    sandboxAttribute(next.permissions.includes('script'), next.dragMode),
  );
  handle.hidden = next.dragMode === 'all';
  if (reload || sandboxChanged) {
    dragSession.cancel();
    setPointerTracking(false);
    pendingRevision = next.revision;
    frame.src = widgetUrl(origin, next.id, next.revision, next.token);
  }
  document.title = next.title || 'aidget';
  hostSurface.hidden = !next.hostSurface;
  chrome.dataset.surface = next.hostSurface ? 'host' : 'none';
  chrome.dataset.state = 'ready';
  fault.hidden = true;

  // The document may already be running: repeat the declaration on every
  // state push, because a reloaded iframe starts from an empty list.
  postToWidget(permissionsMessage(next.permissions));
  postDragConfig();
}

function setPointerTracking(enabled: boolean) {
  if (pointerEnabled === enabled) {
    return;
  }
  pointerEnabled = enabled;
  pointerToggle = pointerToggle
    .then(() => invoke<void>('widget_pointer_tracking', { enabled }))
    .catch((error) => {
      console.error('pointer tracking failed', error);
      if (pointerEnabled === enabled) {
        pointerEnabled = false;
      }
    });
}

function postDragConfig() {
  if (state) {
    postToWidget({
      source: 'aidget-host',
      type: 'drag-config',
      revision: state.revision,
      token: state.token,
      dragMode: state.dragMode,
    });
  }
}

function showFault(message: string) {
  chrome.dataset.state = 'fault';
  hostSurface.hidden = false;
  faultMessage.textContent = message;
  fault.hidden = false;
}

frame.addEventListener('error', () => {
  setPointerTracking(false);
  showFault('无法读取小组件文件');
});
frame.addEventListener('load', () => {
  if (pendingRevision) {
    void invoke('widget_loaded', { revision: pendingRevision }).catch(() => {
      // A missing listener only costs screenshot freshness.
    });
    pendingRevision = 0;
  }
  // Every navigation starts a fresh document with an empty declaration, so
  // push it on each load rather than only the ones this page triggered.
  if (state) {
    postToWidget(permissionsMessage(state.permissions));
  }
  postDragConfig();
});

async function bootstrap() {
  try {
    const next = await invoke<WidgetState>('widget_state');
    apply(next);
  } catch (error) {
    showFault(String(error));
  }
}

handle.addEventListener('pointerdown', (event) => {
  if (event.button !== 0 || !event.isPrimary || event.pointerType !== 'mouse') {
    return;
  }
  event.preventDefault();
  void startDrag().catch((error) => showFault(String(error)));
});

async function startDrag(signal: DragSignal | null = null) {
  if (dragSession.active) {
    return;
  }
  activeDrag = signal;
  try {
    await dragSession.start(
      () =>
        !signal ||
        (state?.revision === signal.revision &&
          state.token === signal.token &&
          state.dragMode === 'all'),
    );
  } finally {
    if (signal) {
      postToWidget({ ...signal, source: 'aidget-host', type: 'drag-end' });
    }
    activeDrag = null;
  }
}

window.addEventListener('blur', () => dragSession.cancel());
window.addEventListener('pagehide', () => dragSession.cancel());

// Host side of the widget bridge: only the iframe this window owns may ask,
// and only for methods the host actually implements.
window.addEventListener('message', (event) => {
  const drag = parseDragSignal(event, frame.contentWindow, state);
  if (drag) {
    if (drag.phase === 'cancel') {
      if (activeDrag?.gesture === drag.gesture) {
        dragSession.cancel();
      }
    } else {
      void startDrag(drag).catch((error) => showFault(String(error)));
    }
    return;
  }
  const pointer = parsePointerSubscription(event.data, event.source, frame.contentWindow);
  if (pointer) {
    if (state?.revision === pointer.revision && state.token === pointer.token) {
      setPointerTracking(pointer.enabled);
    }
    return;
  }
  const ready = parseReady(event.data, event.source, frame.contentWindow);
  if (ready) {
    if (state?.revision === ready.revision) {
      void invoke('widget_ready', { revision: ready.revision }).catch(() => {
        // A missing listener only costs screenshot freshness.
      });
    }
    return;
  }
  const request = parseRequest(event.data, event.source, frame.contentWindow);
  if (!request) {
    return;
  }
  const route = routeRequest(request.method, request.params);
  if (!route) {
    postToWidget(errorMessage(request.id, { code: 'invalid_params', message: 'unknown method' }));
    return;
  }
  if ('error' in route) {
    postToWidget(errorMessage(request.id, route.error));
    return;
  }
  void invoke<unknown>(route.command, route.args)
    .then((result) => postToWidget(resultMessage(request.id, result)))
    .catch((error) => postToWidget(errorMessage(request.id, failureOf(error))));
});

function postToWidget(message: unknown) {
  frame.contentWindow?.postMessage(message, '*');
}

let persistTimer: number | undefined;

void appWindow.onMoved(() => {
  window.clearTimeout(persistTimer);
  persistTimer = window.setTimeout(() => {
    void invoke('persist_window_position').catch((error) => {
      showFault(String(error));
    });
  }, 180);
});

void listen<WidgetState>(
  'aidget://widget',
  (event) => {
    try {
      apply(event.payload);
    } catch (error) {
      showFault(String(error));
    }
  },
  { target: widgetEventTarget(appWindow.label) },
);

void listen<string>(
  'aidget://fault',
  (event) => {
    showFault(event.payload);
  },
  { target: widgetEventTarget(appWindow.label) },
);

void listen<unknown>(
  'aidget://pointer',
  (event) => {
    if (!pointerEnabled || !state) {
      return;
    }
    const pointer = parsePointerEvent(event.payload);
    if (pointer) {
      postToWidget(pointerMessage(pointer, state.revision, state.token));
    }
  },
  { target: widgetEventTarget(appWindow.label) },
);

(window as unknown as { __aidgetApply?: (next: WidgetState) => void }).__aidgetApply = apply;

void bootstrap();
