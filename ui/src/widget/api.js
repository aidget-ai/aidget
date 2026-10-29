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
  var revisionMatch = /(?:^|[?&])v=(\d+)-/.exec(location.search || '');
  var revision = revisionMatch ? Number(revisionMatch[1]) : 0;

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
    settle(data);
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
