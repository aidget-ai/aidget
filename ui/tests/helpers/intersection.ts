import { vi } from 'vite-plus/test';

/** Deterministic visibility notifications for media scheduling tests. */
export function mockIntersections(automatic = true) {
  const observers: Observer[] = [];
  class Observer implements IntersectionObserver {
    readonly root: Element | Document | null;
    readonly rootMargin: string;
    readonly thresholds = [0];
    readonly scrollMargin = '0px';
    readonly targets = new Set<Element>();
    constructor(
      readonly callback: IntersectionObserverCallback,
      options: IntersectionObserverInit = {},
    ) {
      this.root = options.root ?? null;
      this.rootMargin = options.rootMargin ?? '0px';
      observers.push(this);
    }
    observe(target: Element) {
      this.targets.add(target);
      if (automatic) {
        queueMicrotask(() => {
          if (this.targets.has(target)) {
            this.notify(target, true);
          }
        });
      }
    }
    unobserve(target: Element) {
      this.targets.delete(target);
    }
    disconnect() {
      this.targets.clear();
    }
    takeRecords() {
      return [];
    }
    notify(target: Element, intersecting: boolean) {
      const rect = target.getBoundingClientRect();
      this.callback(
        [
          {
            target,
            isIntersecting: intersecting,
            intersectionRatio: intersecting ? 1 : 0,
            boundingClientRect: rect,
            intersectionRect: rect,
            rootBounds: rect,
            time: 0,
          },
        ],
        this,
      );
    }
  }
  vi.stubGlobal('IntersectionObserver', Observer);
  return {
    observers,
    notify(this: void, target: Element, visible = true, near = visible) {
      for (const observer of observers) {
        if (observer.targets.has(target)) {
          observer.notify(target, observer.rootMargin === '300px' ? near : visible);
        }
      }
    },
  };
}
