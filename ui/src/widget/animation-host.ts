import { HOST_SOURCE, WIDGET_SOURCE } from './bridge';
import type { WidgetState } from './url';

/** One frame batch owned by a particular widget document, including same-revision reloads. */
export interface AnimationRequest {
  type: 'animation-frame-request' | 'animation-frame-cancel';
  revision: number;
  token: string;
  session: string;
  request: number;
}

/** Validate both the document identity and the script grant before scheduling work. */
export function parseAnimationRequest(
  event: Pick<MessageEvent, 'data' | 'source'>,
  frame: Window | null,
  state: WidgetState | null,
): AnimationRequest | null {
  if (!frame || event.source !== frame || !state) {
    return null;
  }
  const data: unknown = event.data;
  if (!data || typeof data !== 'object') {
    return null;
  }
  const candidate = data as Partial<AnimationRequest> & { source?: unknown };
  if (
    candidate.source !== WIDGET_SOURCE ||
    (candidate.type !== 'animation-frame-request' && candidate.type !== 'animation-frame-cancel') ||
    candidate.revision !== state.revision ||
    candidate.token !== state.token ||
    typeof candidate.session !== 'string' ||
    !candidate.session ||
    candidate.session.length > 128 ||
    typeof candidate.request !== 'number' ||
    !Number.isSafeInteger(candidate.request) ||
    candidate.request <= 0 ||
    (candidate.type === 'animation-frame-request' && !state.permissions.includes('script'))
  ) {
    return null;
  }
  return {
    type: candidate.type,
    revision: state.revision,
    token: state.token,
    session: candidate.session,
    request: candidate.request,
  };
}

type AnimationMessage = Omit<AnimationRequest, 'type'> & {
  source: typeof HOST_SOURCE;
  type: 'animation-frame';
  timestamp: number;
};
type FrameClock = Pick<Window, 'requestAnimationFrame' | 'cancelAnimationFrame'> & {
  performance: Pick<Performance, 'timeOrigin'>;
};

/** Keep at most one frame in flight; hidden shells retain their request until visible. */
export function createAnimationHost(
  post: (message: AnimationMessage) => void,
  clock: FrameClock = window,
) {
  let pending: AnimationRequest | null = null;
  let handle: number | null = null;
  let visible = true;

  function cancelScheduled() {
    if (handle !== null) {
      clock.cancelAnimationFrame(handle);
      handle = null;
    }
  }

  function schedule() {
    if (!visible || !pending || handle !== null) {
      return;
    }
    const request = pending;
    handle = clock.requestAnimationFrame((timestamp) => {
      if (pending !== request) {
        return;
      }
      handle = null;
      pending = null;
      post({
        ...request,
        source: HOST_SOURCE,
        type: 'animation-frame',
        timestamp: clock.performance.timeOrigin + timestamp,
      });
    });
  }

  return {
    accept(request: AnimationRequest) {
      if (request.type === 'animation-frame-cancel') {
        if (
          pending?.session === request.session &&
          pending.request === request.request &&
          pending.revision === request.revision &&
          pending.token === request.token
        ) {
          cancelScheduled();
          pending = null;
        }
        return;
      }
      cancelScheduled();
      pending = request;
      schedule();
    },
    setVisible(next: boolean) {
      visible = next;
      if (!visible) {
        cancelScheduled();
      } else {
        schedule();
      }
    },
    reset() {
      cancelScheduled();
      pending = null;
    },
  };
}
