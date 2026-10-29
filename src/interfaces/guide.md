# Aidget Widget Guide

Version {{VERSION}}

This is the canonical authoring contract for Aidget widgets. Read it before
creating a widget or changing its layout, scripts, permissions, network access
or rendered surface.

## Data root

The default root is the OS home directory's `.aidget` folder. `AIDGET_HOME`
overrides it. On Windows the default is `C:\Users\<user>\.aidget`.

```text
<root>/
├── board.json
├── settings.toml
├── run/
├── screenshots/
├── storage/
├── trash/
└── widgets/<id>/
    ├── config.json
    ├── index.html
    ├── style.css
    ├── agents.md
    └── assets/
```

Do not write these files directly. CLI and MCP writes maintain revisions and
notify the resident UI. A direct file write can race the watcher.

## Widget identity and layout

- IDs match `[a-z0-9][a-z0-9-]{0,63}`.
- Aidget supports up to 50 widgets.
- Widget position belongs to the user. Content writes never move a window.
- Content writes use the widget revision as `baseRevision`.
- `board move` uses the independent board revision.
- A drag or resize bumps `boardRevision` without a tool call.
- On a board conflict, `boardChangedByUser` means re-read the board rather than
  restoring an old frame.

## config.json

```json
{
  "id": "note",
  "title": "Note",
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
- `size` is the initial logical size and must be between 160x120 and
  8192x8192.
- `permissions` accepts `script`, `location` and `storage`.
- Every `network` entry must be an exact HTTPS origin such as
  `https://api.example.com`. Paths, queries, fragments, credentials, HTTP and
  wildcard hosts are invalid.
- Installed copies additionally contain host-managed `revision` and
  `updatedAt`; do not create those fields yourself.

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
method. Calls time out after 25 seconds unless the method documents a shorter
limit.

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

## Platform capabilities

| Capability     | macOS               | Windows             | Linux                                                               |
| -------------- | ------------------- | ------------------- | ------------------------------------------------------------------- |
| Screenshot     | WKWebView           | WebView2 DevTools   | WebKitGTK                                                           |
| Location       | CoreLocation        | WinRT Geolocation   | GeoClue                                                             |
| Native glass   | AppKit material     | Not available       | Not available                                                       |
| Network bridge | Native HTTPS client | Native HTTPS client | Native HTTPS client                                                 |
| Placement      | User-controlled     | User-controlled     | Under Wayland, the compositor controls stacking and exact placement |

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
`permission_denied`, `denied`, `io_error`, `timeout`, `http_error`,
`invalid_response`, `unavailable`.
