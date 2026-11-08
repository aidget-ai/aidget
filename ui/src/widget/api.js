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
  var diagnosticEnabled = false;
  var diagnosticQueue = [];
  var diagnosticFlushTimer = null;
  var diagnosticDropTimer = null;
  var diagnosticDropped = 0;

  function bounded(value, limit) {
    var text;
    try {
      if (value instanceof Error) {
        text = value.name + ': ' + value.message;
      } else {
        text = typeof value === 'string' ? value : String(value);
      }
    } catch {
      text = 'unserializable diagnostic value';
    }
    return text.length > limit ? text.slice(0, limit) : text;
  }

  function cleanLocation(value) {
    if (typeof value !== 'string') {
      return null;
    }
    return value.split('#')[0].split('?')[0].slice(0, 8192) || null;
  }

  function diagnostic(source, value, details) {
    if (!Number.isSafeInteger(revision) || revision <= 0 || !token) {
      return;
    }
    var info = details || {};
    var message;
    var stack = null;
    if (value instanceof Error) {
      message = value.message;
      stack = value.stack;
    } else {
      message = bounded(value, 8192);
      if (value && typeof value === 'object') {
        try {
          message = JSON.stringify(value).slice(0, 8192);
        } catch {
          message = bounded(value, 8192);
        }
      }
    }
    var event = {
      source: source,
      revision: revision,
      message: bounded(message || source, 8192),
      stack: stack ? bounded(stack, 8192) : null,
      filename: cleanLocation(info.filename),
      line: Number.isSafeInteger(info.line) && info.line >= 0 ? info.line : null,
      column: Number.isSafeInteger(info.column) && info.column >= 0 ? info.column : null,
    };
    var payload = {
      source: 'aidget-widget',
      type: 'diagnostic',
      revision: revision,
      token: token,
      event: event,
    };
    if (diagnosticQueue.length >= 100) {
      diagnosticDropped += 1;
      scheduleDiagnosticDropped();
      return;
    }
    diagnosticQueue.push(payload);
    scheduleDiagnosticFlush(0);
  }

  function reportDiagnosticDropped() {
    diagnosticDropTimer = null;
    if (!diagnosticEnabled || !diagnosticDropped) {
      return;
    }
    window.parent.postMessage(
      {
        source: 'aidget-widget',
        type: 'diagnostic-dropped',
        revision: revision,
        token: token,
        dropped: diagnosticDropped,
      },
      '*',
    );
    diagnosticDropped = 0;
  }

  function scheduleDiagnosticDropped() {
    if (!diagnosticEnabled || diagnosticDropTimer !== null) {
      return;
    }
    diagnosticDropTimer = setTimeout(reportDiagnosticDropped, 1050);
  }

  function flushDiagnosticQueue() {
    diagnosticFlushTimer = null;
    if (!diagnosticEnabled || !diagnosticQueue.length) {
      return;
    }
    diagnosticQueue.splice(0, 20).forEach(function (payload) {
      window.parent.postMessage(payload, '*');
    });
    if (diagnosticQueue.length) {
      scheduleDiagnosticFlush(1050);
    }
  }

  function scheduleDiagnosticFlush(delay) {
    if (!diagnosticEnabled || diagnosticFlushTimer !== null || !diagnosticQueue.length) {
      return;
    }
    diagnosticFlushTimer = setTimeout(flushDiagnosticQueue, delay);
  }

  window.addEventListener(
    'error',
    function (event) {
      if (event.target && event.target !== window) {
        diagnostic('resource_error', event.target.src || event.target.href || 'resource failed', {
          filename: event.target.src || event.target.href,
        });
        return;
      }
      diagnostic('error', event.error || event.message, {
        filename: event.filename,
        line: event.lineno,
        column: event.colno,
      });
    },
    true,
  );
  window.addEventListener('unhandledrejection', function (event) {
    diagnostic('unhandled_rejection', event.reason);
  });
  window.addEventListener('securitypolicyviolation', function (event) {
    diagnostic(
      'csp_violation',
      event.violatedDirective + ' blocked ' + (event.blockedURI || 'resource'),
      { filename: event.sourceFile, line: event.lineNumber, column: event.columnNumber },
    );
  });
  var originalConsoleError = console.error;
  console.error = function () {
    var values = Array.prototype.slice.call(arguments);
    diagnostic(
      'console_error',
      values
        .map(function (value) {
          return bounded(value, 8192);
        })
        .join(' '),
    );
    return originalConsoleError.apply(console, arguments);
  };

  function call(name, params) {
    return new Promise(function (resolve, reject) {
      var id = 'aidget-' + ++seq;
      var timer = setTimeout(function () {
        pending.delete(id);
        var error = new Error('timeout');
        error.code = 'timeout';
        diagnostic('host_api', error, { filename: 'window.aidget.' + name });
        reject(error);
      }, 25000);
      pending.set(id, { resolve: resolve, reject: reject, timer: timer, method: name });
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
    diagnostic('host_api', error, { filename: 'window.aidget.' + entry.method });
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
    if (data.type === 'diagnostics-enabled' && data.revision === revision && data.token === token) {
      diagnosticEnabled = true;
      window.parent.postMessage(
        {
          source: 'aidget-widget',
          type: 'diagnostic-ready',
          revision: revision,
          token: token,
        },
        '*',
      );
      flushDiagnosticQueue();
      scheduleDiagnosticDropped();
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
