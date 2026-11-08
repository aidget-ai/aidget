# Aidget Widget Guide

Version {{VERSION}}

This is the canonical authoring contract for Aidget widgets. Read it before
creating a widget or changing its layout, scripts, permissions, network access
or rendered surface.

## Data root

The optional global CLI option `--home <directory>` selects the data root.
The default is the OS home directory's `.aidget` folder
(`C:\Users\<user>\.aidget` on Windows).

```text
<root>/
├── board.json
├── settings.toml
├── run/
├── state/
│   ├── agent-backups/
│   ├── community/
│   ├── previews/<id>.png
│   ├── screenshots/
│   ├── storage/<id>.json
│   └── migrations/          # completed host metadata migrations
├── skills/aidget/SKILL.md
├── trash/
└── widgets/<id>/
    ├── package.json          # optional npm package metadata
    ├── config.json
    ├── index.html
    ├── style.css
    ├── agents.md
    └── assets/
```

Some entries are created only when needed. `trash/` holds deleted widgets
retained for restoration.

Do not write these files directly. CLI and MCP writes maintain revisions and
notify the resident UI. A direct file write can race the watcher.

## Widget identity and layout

- IDs match `[a-z0-9][a-z0-9-]{0,63}`.
- Aidget supports up to 50 widgets.
- Widget position belongs to the user. Content writes never move a window.
- Content writes use the widget revision as `baseRevision`.
- Enabled-state changes share that revision, including changes from the main window.
- `board move` uses the independent board revision.
- A drag or resize bumps `boardRevision` without a tool call.
- On a board conflict, `boardChangedByUser` means re-read the board rather than
  restoring an old frame.

## config.json

```json
{
  "id": "note",
  "title": "Note",
  "minHostVersion": "0.1.1",
  "logo": null,
  "screenshots": [],
  "platforms": ["macos", "windows", "linux"],
  "size": { "width": 360, "height": 220 },
  "dragMode": "all",
  "permissions": [],
  "network": []
}
```

- `platforms` lists the operating systems on which the widget was checked.
- `minHostVersion` is a concrete SemVer release required by the widget. New
  local widgets default to the version that created them. Community packages
  must declare it. Version ranges, npm tags and a leading `v` are invalid.
- If the running Aidget version is older than `minHostVersion`, the widget is
  blocked from install and from running. Its files, storage and frame remain.
- Historical local widgets without this field continue to run and show as
  undeclared. Community installs without a declaration are rejected.
- `size` is the initial logical size and must be between 160x120 and
  8192x8192.
- `permissions` accepts `script`, `location` and `storage`.
- Every `network` entry must be an exact HTTPS origin such as
  `https://api.example.com`. Paths, queries, fragments, credentials, HTTP and
  wildcard hosts are invalid.
- Installed copies additionally contain host-managed `revision`, `updatedAt`
  and `enabled`; do not put these fields in community packages.
- Unused fields are ignored. Missing required fields and invalid current values
  are rejected; unused fields never grant permissions or supply missing values.

`aidget_create` and `aidget_update` accept `minHostVersion`. Update omits the
field to preserve it. A patch never changes it.

## Enable and disable widgets

Use `aidget_update` with optional `enabled: true|false`, or:

```sh
aidget widget update note --enabled false --revision 3 --json
aidget widget update note --enabled true --revision 4 --json
```

Omitted `enabled` preserves the current state. Disabling closes the desktop
window and retains files, frame and cached preview. Enabling reuses the frame,
or creates a default frame with the configured size if missing. Patches and
moves preserve enabled state.

`list` and write acknowledgements include `enabled`; `get` returns `meta.enabled`.
This is the persisted target state; native window reconciliation is asynchronous.
Toggles preserve the frame and board revision unless a missing frame is created.
Update acknowledgements always omit the frame.

Setting only the current enabled value is a no-op after revision validation.
An enabled-state change increments the widget revision once, even when combined
with content changes.
Stale requests fail even if the requested state matches; re-read before retrying.

CLI updates work offline and take effect through the running GUI's watcher or
at its next launch.
MCP starts the GUI on demand. Enabled state and revision commit together; content
files commit individually, so failed mixed updates may leave partial content changes.

Stop older Aidget processes before upgrading a data directory. Startup backs up
the old archive map to `state/migrations/enabled/widgets.json`; downgrading does
not restore it.

## Regions

Mark replaceable content with a unique `data-aidget-region="name"` attribute.
Names must be unique within the widget.

Patches replace inner HTML only. Target a container such as `div`, `ul` or
`section`, not a text-only element whose outer tag must remain. Passing several
regions in one patch is atomic: if any target is missing or ambiguous, no file
changes.

## Visual surfaces

The host surface is a static fallback card by default. To paint every pixel,
declare:

```html
<meta name="aidget-surface" content="none" />
```

On macOS, `content="glass"` places an AppKit material behind the widget. Keep
the page background transparent when using glass. Avoid CSS `backdrop-filter`
for essential readability because desktop blur can flash transparent while
dragging.

## Dragging

`dragMode` defaults to `all`. Ordinary content starts a native drag after 4px,
while controls remain interactive.

Use `handle` to restrict dragging to the host's top 28px handle. Mark selectable
text and custom interaction containers with `data-aidget-no-drag`; descendants
inherit the exception.

The CLI accepts `--drag-mode all|handle`. An update that omits the option keeps
the current mode.

## Scripts and resources

JavaScript requires the `script` permission. Scripts run in an opaque-origin
iframe without same-origin access to Aidget or other widgets.

- Use package-local scripts or existing inline `<script>` blocks. The host
  hashes inline blocks when serving the document.
- Remote scripts, `eval`, `new Function`, inline event attributes and
  `javascript:` URLs are blocked. Use `addEventListener`.
- Nested frames, objects and workers are blocked.
- `style.css` and `assets/` remain local package resources.
- External images, styles, fonts and media require their exact HTTPS origin in
  `network`. A network grant never authorizes remote JavaScript.
- Keep runtime paths relative and inside the widget package.

## agents.md

`agents.md` is never rendered. It records durable author guidance: editable
regions, layout constraints, refresh behavior and data-source rules.

Read it through the widget document before editing. If local markup or a new
request conflicts with it, keep the guidance and change the plan unless the user
explicitly overrides it. Update it whenever stable regions, layout constraints
or refresh behavior change.

## window.aidget bridge

Every bridge method below requires `script`. Additional grants are listed per
method. Request/response calls time out after 25 seconds unless the method documents
a shorter limit. Pointer subscriptions and animation frames have no request timeout.

### `ready()`

```js
window.aidget.ready();
```

Optional, idempotent readiness signal. Call it after asynchronous initialization
and the final DOM update. Screenshot capture waits up to 500ms for this signal.

### `permissions`

```js
window.aidget.permissions;
```

Read-only array snapshot of the current declaration. It can initially be empty.
Use it for display or branching, never as the authorization gate.

### `onPointer(handler)`

```js
const stop = window.aidget.onPointer(({ type, x, y }) => {
  // type: "enter" | "move" | "leave"
});
```

Subscribes to pointer hover for this widget without requiring the desktop
window to be active. Each call returns an independent, idempotent `stop()`
function. Multiple listeners share one host subscription.

`x` and `y` are CSS pixels relative to the widget viewport, with the origin at
the top-left. `leave` carries the last in-window position. A listener that
throws does not prevent other listeners from running.

The host emits `enter`, `move`, and `leave` only. It does not report clicks,
buttons, wheel input, touch, or pen. It also does not synthesize CSS `:hover`
or a DOM `PointerEvent`; use this API to drive styles when the window is
inactive. Clicking still follows normal window activation behavior.

The tracking region is the full widget window, including transparent areas.
An occluded widget receives nothing. When a listener subscribes while the
pointer is already inside, no event is invented immediately; the next pointer
movement starts with `enter`.

### `requestAnimationFrame(callback)` and `cancelAnimationFrame(handle)`

```js
const handle = window.aidget.requestAnimationFrame((timestamp) => {
  // Update animation styles using the document-relative timestamp.
});
window.aidget.cancelAnimationFrame(handle);
```

Requests one animation callback using the widget shell's rendering clock. This
avoids WebKit's reduced frame rate for sandboxed iframes that have not received
a user interaction. The browser's own `requestAnimationFrame` remains available.

The returned handle belongs to this document and can be cancelled repeatedly.
The callback receives milliseconds relative to this document's
`performance.timeOrigin`, like `performance.now()`. Callbacks in one batch share
a timestamp; callbacks requested during that batch run in a later frame. A
callback that throws does not prevent the remaining callbacks from running.

Only pending callbacks generate frame messages. Stop requesting frames when an
animation settles. Hidden shell documents pause pending frames until visible;
unloading the widget cancels its pending callbacks. Window blur does not pause
this API. Actual frame rate depends on the WebView, display and system load.
Use elapsed time for interpolation and cap long gaps when resuming.

### `requestLocation()`

```js
await window.aidget.requestLocation();
```

Requires `location` in addition to `script`. Resolves to:

```js
{
  (latitude, longitude, accuracy, place);
}
```

`accuracy` is in metres. `place` can be empty when reverse geocoding is
unavailable and on Linux. The request prompts through the OS and times out after
20 seconds without a fix.

Errors: `permission_denied`, `denied`, `unavailable`, `timeout`. Never substitute
IP geolocation.

### `storage`

Requires `storage` in addition to `script`.

```js
await window.aidget.storage.getItem(key); // string | null
await window.aidget.storage.setItem(key, value); // null
await window.aidget.storage.removeItem(key); // null
await window.aidget.storage.clear(); // null
```

Keys cannot be empty. Keys and values must be strings. Storage is isolated per
widget and limited to 256 KiB after serialization.

Errors: `permission_denied`, `invalid_params`, `io_error`, `timeout`,
`unavailable`.

### `fetch(url, options?)`

```js
const data = await window.aidget.fetch(url, {
  method: 'GET',
  headers: {},
  body: null,
});
```

Performs a host-side HTTP request and returns the response parsed as JSON. The
URL must be absolute HTTPS with no credentials. Its origin must exactly match an
entry in `network`.

Options:

- `method`: optional HTTP method, default `GET`. `CONNECT`, `TRACE`, `TRACK` and
  `HEAD` are forbidden.
- `headers`: optional object mapping string header names to string values.
  Host-managed transport headers such as `Host` and `Content-Length` are
  rejected. No `Accept` or `Referer` header is added automatically.
- `body`: optional UTF-8 string. GET requests cannot have a body.

Limits:

- request timeout: 12 seconds
- connect timeout: 5 seconds
- response body: 1 MiB
- request body: 256 KiB
- application headers: 32 KiB
- redirects: at most 3, all on the original origin

Fragments are accepted but not sent in the HTTP request. Only 2xx responses
succeed. Non-2xx responses fail with `http_error`; malformed JSON and oversized
responses fail with `invalid_response`.

Errors: `permission_denied`, `invalid_params`, `timeout`, `http_error`,
`invalid_response`, `unavailable`.

### `openUrl(url)`

```js
await window.aidget.openUrl(url);
```

Opens an allowed URL in the system browser. The URL must be absolute HTTPS with
no credentials, and its origin must exactly match an entry in `network`.
Fragments are preserved.

Errors: `permission_denied`, `invalid_params`, `timeout`, `unavailable`.

## Data refresh

Aidget does not recompute widget data. Use `window.aidget.fetch()` from a
script-enabled widget, or let an Agent update content through the tools.

The main window recaptures enabled previews every 30 seconds while the
local-widgets page is visible. That preview timer does not refresh external API
data by itself.

## Screenshots

`scale` is output pixels per layout point, independent of display pixel density.
The default scale is 1; use 2 only when detail matters.

Capture waits for the current iframe load and then up to 500ms for
`window.aidget.ready()`. Slow initialization intentionally produces an early
capture.

## Runtime diagnostics

As an optional final verification step after taking a screenshot, query the
session diagnostics for the widget's current revision:

```sh
aidget widget diagnostics note --json
```

Use `aidget_diagnostics` from MCP. Records cover uncaught JavaScript errors,
unhandled Promise rejections, calls to `console.error`, resource and CSP
failures, and failed `window.aidget` requests. Each widget retains at most 100
records per host session. Errors are cleared when Aidget exits or the widget is
deleted.

The result reports its session, revision, next cursor, dropped count and whether
the collector is connected. No records means no observable error was collected;
it does not prove that the widget is healthy. Errors caught by widget code and
never logged cannot be detected automatically.
Diagnostics are available through CLI and MCP only; the management window does
not expose a diagnostics view.

## Platform capabilities

| Capability     | macOS               | Windows             | Linux                                                               |
| -------------- | ------------------- | ------------------- | ------------------------------------------------------------------- |
| Screenshot     | WKWebView           | WebView2 DevTools   | WebKitGTK                                                           |
| Location       | CoreLocation        | WinRT Geolocation   | GeoClue                                                             |
| Native glass   | AppKit material     | Not available       | Not available                                                       |
| Pointer hover  | AppKit tracking     | Win32 cursor track  | GTK motion events                                                   |
| Network bridge | Native HTTPS client | Native HTTPS client | Native HTTPS client                                                 |
| Placement      | User-controlled     | User-controlled     | Under Wayland, the compositor controls stacking and exact placement |

New widgets are placed into the first free slot inside a display's work area,
preferring the default display chosen in the main window's settings, then the
primary display, and falling back to another display when needed.
The CLI cannot measure displays, so with a default display chosen, a CLI-created
widget is placed this way when the app first opens it, unless it was moved first.
Later moves are user-owned and take precedence over automatic placement.

On Windows, widget content is served from `http://aidget.localhost`. Native
`fetch` still requires an HTTPS API origin.

List only platforms actually checked and rendered in `config.json`.

## Packages

`package.json` is optional for local widgets. Community packages use
`@aidget-community/<id>` and include config, HTML, CSS, agents guidance, package
metadata and runtime assets.

Do not add build steps, lifecycle scripts or runtime dependencies. A malformed
package file is an error. Local cards do not display provenance badges.

## Limits and retention

- maximum widgets: 50
- maximum patch payload: 256 KiB
- default screenshot retention: 30
- default trash retention: 20

## Error codes

`ui_not_running`, `not_found`, `invalid_id`, `invalid_params`,
`revision_conflict`, `region_not_found`, `region_ambiguous`,
`host_version_incompatible`, `host_version_required`, `permission_denied`,
`denied`, `io_error`, `timeout`, `http_error`, `invalid_response`,
`unavailable`.
