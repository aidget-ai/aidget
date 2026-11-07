// Injected into every widget document as `window.aidget`.
//
// It is plain JavaScript, not a module, because the host evaluates it before
// the document's own scripts. It only installs itself inside `/content/`
// documents served by the aidget host: the shell page and the main-window
// preview also run this script. Windows serves the same files over
// `http://aidget.localhost`, so the host is checked instead of the scheme.
(function () {
  if (!location.pathname.startsWith('/content/')) {
    return;
  }
  if (location.hostname !== 'localhost' && location.hostname !== 'aidget.localhost') {
    return;
  }

  var pending = new Map();
  var seq = 0;
  var declared = [];
  var readySent = false;
  var pointerListeners = [];
  var animationCallbacks = new Map();
  var animationHandle = 0;
  var animationSequence = 0;
  var animationRequest = 0;
  var animationSession = '';
  var deliveringAnimation = false;
  var versionMatch = /(?:^|[?&])v=(\d+)-([^&]+)/.exec(location.search || '');
  var revision = versionMatch ? Number(versionMatch[1]) : 0;
  var token = versionMatch ? versionMatch[2] : '';

  function call(name, params) {
    return new Promise(function (resolve, reject) {
      var id = 'aidget-' + ++seq;
      var timer = setTimeout(function () {
        pending.delete(id);
        var error = new Error('timeout');
        error.code = 'timeout';
        reject(error);
      }, 25000);
      pending.set(id, { resolve: resolve, reject: reject, timer: timer });
      window.parent.postMessage(
        { source: 'aidget-widget', id: id, method: name, params: params },
        '*',
      );
    });
  }

  function settle(data) {
    var entry = pending.get(data.id);
    if (!entry) {
      return;
    }
    pending.delete(data.id);
    clearTimeout(entry.timer);
    if (data.ok) {
      entry.resolve(data.result);
      return;
    }
    var error = new Error((data.error && data.error.message) || 'request failed');
    error.code = (data.error && data.error.code) || 'unavailable';
    entry.reject(error);
  }

  function pointerSubscription(enabled) {
    if (!Number.isSafeInteger(revision) || revision <= 0 || !token) {
      return;
    }
    window.parent.postMessage(
      {
        source: 'aidget-widget',
        type: 'pointer-subscription',
        enabled: enabled,
        revision: revision,
        token: token,
      },
      '*',
    );
  }

  function animationSignal(type) {
    window.parent.postMessage(
      {
        source: 'aidget-widget',
        type: type,
        revision: revision,
        token: token,
        session: animationSession,
        request: animationRequest,
      },
      '*',
    );
  }

  function scheduleAnimation() {
    if (deliveringAnimation || animationRequest || !animationCallbacks.size) {
      return;
    }
    animationRequest = ++animationSequence;
    animationSignal('animation-frame-request');
  }

  function cancelAnimation() {
    if (animationRequest) {
      animationSignal('animation-frame-cancel');
      animationRequest = 0;
    }
  }

  window.addEventListener('message', function (event) {
    if (event.source !== window.parent) {
      return;
    }
    var data = event.data;
    if (!data || data.source !== 'aidget-host') {
      return;
    }
    if (data.type === 'permissions') {
      declared = Array.isArray(data.permissions) ? data.permissions.slice() : [];
      return;
    }
    if (data.type === 'animation-frame') {
      if (
        !animationRequest ||
        data.revision !== revision ||
        data.token !== token ||
        data.session !== animationSession ||
        data.request !== animationRequest ||
        !Number.isFinite(data.timestamp) ||
        data.timestamp < performance.timeOrigin
      ) {
        return;
      }
      animationRequest = 0;
      deliveringAnimation = true;
      // Snapshot IDs so re-entrant requests wait for the next frame and cancellation still works.
      var handles = Array.from(animationCallbacks.keys());
      var timestamp = data.timestamp - performance.timeOrigin;
      handles.forEach(function (handle) {
        var callback = animationCallbacks.get(handle);
        if (!callback) {
          return;
        }
        animationCallbacks.delete(handle);
        try {
          callback(timestamp);
        } catch (error) {
          console.error(error);
        }
      });
      deliveringAnimation = false;
      scheduleAnimation();
      return;
    }
    if (data.type === 'pointer') {
      if (data.revision !== revision || data.token !== token) {
        return;
      }
      var pointer = data.event;
      if (
        !pointer ||
        (pointer.type !== 'enter' && pointer.type !== 'move' && pointer.type !== 'leave') ||
        !Number.isFinite(pointer.x) ||
        !Number.isFinite(pointer.y)
      ) {
        return;
      }
      var delivered = Object.freeze({ type: pointer.type, x: pointer.x, y: pointer.y });
      pointerListeners.slice().forEach(function (entry) {
        if (!entry.active) {
          return;
        }
        try {
          entry.handler(delivered);
        } catch (error) {
          console.error(error);
        }
      });
      return;
    }
    settle(data);
  });

  window.addEventListener('pagehide', function () {
    animationCallbacks.clear();
    cancelAnimation();
    if (!pointerListeners.length) {
      return;
    }
    pointerListeners = [];
    pointerSubscription(false);
  });

  var api = {
    // Optional readiness signal for screenshots; repeated calls are ignored.
    ready: function () {
      if (readySent || revision <= 0) {
        return;
      }
      readySent = true;
      window.parent.postMessage(
        { source: 'aidget-widget', type: 'ready', revision: revision },
        '*',
      );
    },
    requestLocation: function () {
      return call('location');
    },
    fetch: function (url, options) {
      return call('fetch', { url: url, options: options === undefined ? null : options });
    },
    openUrl: function (url) {
      return call('open_url', { url: url });
    },
    // One-shot frames use the shell's clock without replacing the browser's API.
    requestAnimationFrame: function (callback) {
      if (typeof callback !== 'function') {
        throw new TypeError('requestAnimationFrame requires a function');
      }
      if (!Number.isSafeInteger(revision) || revision <= 0 || !token) {
        throw new Error('animation frames require a versioned widget document');
      }
      if (!animationSession) {
        // A fresh nonce distinguishes reloads of the same content revision.
        animationSession = Array.from(crypto.getRandomValues(new Uint32Array(4))).join('-');
      }
      var handle = ++animationHandle;
      animationCallbacks.set(handle, callback);
      scheduleAnimation();
      return handle;
    },
    cancelAnimationFrame: function (handle) {
      animationCallbacks.delete(handle);
      if (!animationCallbacks.size) {
        cancelAnimation();
      }
    },
    onPointer: function (handler) {
      if (typeof handler !== 'function') {
        throw new TypeError('onPointer requires a function');
      }
      var entry = { handler: handler, active: true };
      pointerListeners.push(entry);
      if (pointerListeners.length === 1) {
        pointerSubscription(true);
      }
      return function () {
        if (!entry.active) {
          return;
        }
        entry.active = false;
        var index = pointerListeners.indexOf(entry);
        if (index >= 0) {
          pointerListeners.splice(index, 1);
        }
        if (!pointerListeners.length) {
          pointerSubscription(false);
        }
      };
    },
    storage: Object.freeze({
      getItem: function (key) {
        return call('storage_get', { key: key });
      },
      setItem: function (key, value) {
        return call('storage_set', { key: key, value: value });
      },
      removeItem: function (key) {
        return call('storage_remove', { key: key });
      },
      clear: function () {
        return call('storage_clear');
      },
    }),
  };

  // `api` is frozen, so the declaration is exposed through a getter rather
  // than an assignment that would silently fail.
  Object.defineProperty(api, 'permissions', {
    enumerable: true,
    get: function () {
      return declared.slice();
    },
  });

  Object.defineProperty(window, 'aidget', {
    value: Object.freeze(api),
    configurable: false,
    writable: false,
  });
})();
