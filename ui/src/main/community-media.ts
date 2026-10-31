import { invoke } from '@tauri-apps/api/core';
import { createElement, LayoutGrid } from 'lucide';

import type { CommunityEntry } from '../types/rust';
import { translate, type Locale } from './locale';
import { resolvedLocale } from './preferences';

type Source = { commit: string; package: string; path: string };
type Job = {
  state: 'queued' | 'running' | 'ready' | 'failed';
  refresh: boolean;
  retrySource?: Source;
  url?: string;
  error?: string;
  subscribers: Set<Subscriber>;
};
type Subscriber = {
  source: Source;
  near: boolean;
  visible: boolean;
  detail: boolean;
  paint: (job: Job) => void;
};

/** Load only visible/nearby verified media, sharing four request slots across list and dialog. */
export function createCommunityMedia(locale: () => Locale) {
  const jobs = new Map<string, Job>();
  const views = new Set<{ render: () => void }>();
  let active = true;
  let running = 0;
  let scheduled = false;
  const t = (key: Parameters<typeof translate>[1]) => translate(resolvedLocale(locale()), key);

  function schedule() {
    if (scheduled) {
      return;
    }
    scheduled = true;
    queueMicrotask(() => {
      scheduled = false;
      const viewPriority = (view: Subscriber) =>
        !active || !view.near ? Infinity : view.visible ? (view.detail ? 0 : 1) : 2;
      const priority = (job: Job) => Math.min(...[...job.subscribers].map(viewPriority));
      const queue = [...jobs.entries()]
        .filter(([, job]) => job.state === 'queued' && priority(job) < Infinity)
        .sort((a, b) => priority(a[1]) - priority(b[1]));
      for (const [key, job] of queue) {
        if (running === 4) {
          break;
        }
        running++;
        job.state = 'running';
        const source =
          job.retrySource ??
          [...job.subscribers].sort((a, b) => viewPriority(a) - viewPriority(b))[0].source;
        void invoke<string>('community_media', {
          ...source,
          ...(job.refresh ? { refresh: true } : {}),
        })
          .then(
            (url) => {
              job.url = url;
              job.state = 'ready';
            },
            (error: unknown) => {
              job.error = String(error);
              job.state = 'failed';
            },
          )
          .finally(() => {
            running--;
            for (const view of job.subscribers) {
              view.paint(job);
            }
            if (!job.subscribers.size && job.state !== 'failed') {
              jobs.delete(key);
            }
            schedule();
          });
      }
    });
  }

  function create(
    entry: CommunityEntry,
    path: string,
    commit: string,
    root: Element,
    detail = false,
    logo = false,
  ) {
    const element = document.createElement('div');
    element.className = logo ? 'community-logo' : 'community-media';
    const stage = document.createElement('div');
    stage.className = logo ? 'community-logo-stage' : 'preview-stage community-preview';
    const image = document.createElement('img');
    image.className = logo ? '' : 'preview-image';
    image.width = logo ? 32 : 720;
    image.height = logo ? 32 : 440;
    image.decoding = 'async';
    const message = document.createElement('span');
    element.append(stage);
    let key = '';
    let job: Job | undefined;
    let displayed: string | undefined;
    let disposed = false;
    const subscriber: Subscriber = {
      source: { commit, package: entry.entry.package.name, path },
      near: false,
      visible: false,
      detail,
      paint,
    };

    function paint(value: Job) {
      if (value.state === 'ready' && value.url) {
        if (displayed !== value.url) {
          displayed = value.url;
          image.onload = () => {
            if (job === value && value.state === 'ready') {
              stage.replaceChildren(image);
            }
          };
          image.onerror = () => {
            if (job !== value || value.state !== 'ready') {
              return;
            }
            value.state = 'failed';
            value.error = 'Image decoding failed';
            displayed = undefined;
            for (const view of value.subscribers) {
              view.paint(value);
            }
          };
          image.src = value.url;
          if (image.complete && image.naturalWidth > 0) {
            stage.replaceChildren(image);
          }
        }
      } else if (value.state === 'failed') {
        displayed = undefined;
        message.textContent = t(logo ? 'communityLogoFailed' : 'communityScreenshotFailed');
        message.title = value.error ?? '';
        stage.replaceChildren(message);
      } else if (!displayed) {
        message.textContent = t('loading');
        message.title = '';
        if (logo) {
          const placeholder = document.createElement('span');
          placeholder.className = 'community-logo-default';
          placeholder.append(
            createElement(LayoutGrid, { width: 24, height: 24, 'stroke-width': 1.6 }),
          );
          stage.replaceChildren(placeholder);
        } else {
          stage.replaceChildren(message);
        }
      }
    }

    function detach() {
      if (!job) {
        return;
      }
      job.subscribers.delete(subscriber);
      // Preserve failed digests so reopening details can retry unmounted media.
      if (!job.subscribers.size && job.state !== 'running' && job.state !== 'failed') {
        jobs.delete(key);
      }
    }

    function update(next: CommunityEntry, nextPath: string, nextCommit: string) {
      entry = next;
      const file = entry.entry.files.find((file) => file.path === nextPath);
      if (!file) {
        throw new Error(`Media missing from registry digest list: ${nextPath}`);
      }
      const nextKey = file.sha256.toLowerCase();
      image.alt = logo ? '' : entry.entry.config.title;
      if (nextKey !== key) {
        detach();
        key = nextKey;
        image.onload = null;
        image.onerror = null;
        image.removeAttribute('src');
        displayed = undefined;
        stage.replaceChildren(message);
        job = jobs.get(key);
        if (!job) {
          job = {
            state: 'queued',
            refresh: false,
            subscribers: new Set(),
          };
          jobs.set(key, job);
        }
        job.subscribers.add(subscriber);
      }
      subscriber.source = { commit: nextCommit, package: entry.entry.package.name, path: nextPath };
      paint(job!);
      schedule();
    }

    let visible: IntersectionObserver;
    let near: IntersectionObserver;
    function observe() {
      visible = new IntersectionObserver(
        (entries) => {
          for (const observed of entries) {
            subscriber.visible = observed.isIntersecting;
          }
          if (subscriber.visible) {
            subscriber.near = true;
          }
          schedule();
        },
        { root },
      );
      near = new IntersectionObserver(
        (entries) => {
          for (const observed of entries) {
            subscriber.near = observed.isIntersecting;
          }
          schedule();
        },
        { root, rootMargin: '300px' },
      );
      visible.observe(stage);
      near.observe(stage);
    }
    observe();
    update(entry, path, commit);
    const view = {
      element,
      stage,
      update,
      setRoot(value: Element) {
        root = value;
        visible.disconnect();
        near.disconnect();
        observe();
      },
      render() {
        if (job) {
          paint(job);
        }
      },
      dispose() {
        if (disposed) {
          return;
        }
        disposed = true;
        image.onload = null;
        image.onerror = null;
        visible.disconnect();
        near.disconnect();
        detach();
        views.delete(view);
      },
    };
    views.add(view);
    return view;
  }
  return {
    create,
    /** Retry failed declared media once when its pinned details are opened, including unmounted images. */
    retryFailed(entry: CommunityEntry, commit: string) {
      const { config, files, package: pkg } = entry.entry;
      for (const path of [...config.screenshots, ...(config.logo ? [config.logo] : [])]) {
        const file = files.find((file) => file.path === path);
        if (!file) {
          throw new Error(`Media missing from registry digest list: ${path}`);
        }
        const job = jobs.get(file.sha256.toLowerCase());
        if (!job || job.state !== 'failed') {
          continue;
        }
        job.state = 'queued';
        job.refresh = true;
        job.retrySource = { commit, package: pkg.name, path };
        job.url = undefined;
        job.error = undefined;
        for (const view of job.subscribers) {
          view.paint(job);
        }
      }
      schedule();
    },
    /** Suspend queued media while the community panel is hidden. */
    setActive(value: boolean) {
      active = value;
      schedule();
    },
    /** Translate existing states without replacing decoded images. */
    render() {
      for (const view of views) {
        view.render();
      }
    },
  };
}
