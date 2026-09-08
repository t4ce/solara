# BIOS page and startup bookmarks

The initial target is the unmodified read-only BIOS application served at
`http://192.168.178.94:8338/`. Its HTML links `/app.css` and deferred `/app.js`;
the latter requests `/api/bios/schema` and builds the platform overview. No BIOS
HTML, CSS, script or firmware data is embedded in the browser implementation.

The updated kernel and native Blueprint were deployed to the physical rig on
2026-09-08. Navigating to the IP address fetched the original HTML, CSS, deferred
script and schema, then painted the populated System Overview (339 boxes and
2,014 glyphs at 800×512). A fresh WD screenshot confirms the native result.
Keys 9 and 1 filled their addresses without an HTTP request; Ctrl-L accepted the
IP's digits normally, and Enter initiated navigation. The terminal output and
deployment receipt are retained alongside the render evidence.

The host probe executes the same captured script and schema through PageRuntime.
Its Details button test opens the drawer (405 boxes, 2,582 glyphs at 1280×800).
Native pointer dispatch is implemented; this drawer interaction has only been
verified by the host probe. Captures, logs and SVG/PNG evidence live under
`target/bios-proof`, outside git.

The first rig result exposed excessive temporary polygon allocation during
glyph clipping. The painter now classifies geometry bounds against convex clips
and reserves exact triangle clipping for intersecting geometry. The captured
BIOS viewport rasterizes pixel-identically before and after this optimization;
regression tests also cover both clip windings, rotation and degenerate clips.
On the rig, painting the populated overview fell from 153.958 s to 13.164 s;
the complete update tick fell from 160.966 s to 20.172 s. JavaScript/DOM work
took 5.028 s and CSS/layout 1.907 s in the latter run. This remains a slow first
load: these are update timings after the response arrives, not total navigation
latency. Phase timings are logged so later bringup can target the remaining cost.

## Startup and terminal controls

The kernel exposes its public embedded `startup.json` through the repeatable,
read-only `vFile:startup` async-fs read. This is independent of the consumed
`vFile:launch` script. Solara reads `solara.bookmarks` once at launch. Each entry
is a URL string or a `{ "label": "Name", "url": "http://host:port/" }` object.
The first nine entries are used; extras are ignored. Invalid entries reject the
list rather than changing the association between number keys and positions.
Labels have terminal controls removed; targets reject controls and whitespace.
Missing configuration leaves an empty bookmark list and normal address editing.

The default nine entries include BIOS, TRUEOS files, PeerTube browse/home,
the three built-in demos, example.com, and the original CERN web page. Entries
are displayed one per row below the small navbar and clipped on short terminals.

- **1–9** while idle, or clicking a row: load its target into the address bar.
- **Enter**: navigate. Selection alone creates no request.
- **Ctrl-L**: select the address and edit; digits now type normally.
- **Tab**: return to number-key selection. **F2**: toggle HTTP/HTTPS.
- **Esc**: park in Shell2. **Ctrl-Q**: quit this browser.

Updating these defaults on the rig requires a kernel rebuild because the
manifest is embedded, plus a rebuilt Solara Blueprint for the new UI/runtime.

## Local HTTP transport

Local TCP connections now establish a known listening handle, so kernel HTTP
workers and clients in another VM reach the normal server accept queue. This
replaces the old Mio helper that guessed an adjacent handle for same-VM clients.

The kernel HTTP client now preserves a non-default port in Host and uses normal
HTTP/1.1 persistence. Against the BIOS server, requesting Connection: close
reset the connection before the body arrived; persistence completed HTML, CSS,
JavaScript and the 219 KiB schema response. The client still releases its
connection after consuming the response. The capture tool uses the same policy.

## Runtime boundary

A separate QuickJS context owns remote classic scripts. It cannot load the
embedded parser modules or access Blueprint/filesystem/GPU bindings. Initial
DOM identities map to retained Blitz NodeIds. A per-tick journal batches JS/Rust
bridge traffic; create, detach, insert, text and
attribute mutations pass through Blitz's invalidating DocumentMutator. Fragment
HTML is parsed by Parse5 in a separate parser context. There is no alternate
CSS/layout engine and no site-specific fallback content.

The initial DOM surface covers element/text creation, traversal, textContent,
innerHTML, attributes, classList, dataset, simple compound selectors, input
value reflection, bubbling listeners/onclick, focus bookkeeping and timers.
Native primary-click release on the pressed hit target dispatches to the realm.
The next native tick applies mutations, resolves CSS/layout and repaints.
Navigating away drops the realm and pending request handles; a script error is
reported in the TUI while the static document remains available.

Bounds: 32 classic tags; 4 MiB per script/fetch body; 32,768 allocated nodes per
page including detached nodes; 256 insertion depth; 256 timers; 16 queued fetches,
4 in flight and 128 per page; 65,536 mutations/16 MiB per slice. Script evaluations
and Promise slices have a two-second deadline; at most 128 Promise jobs run per tick.
The existing QuickJS memory and stack limits remain enabled. Host-side mutation
validation guards against inconsistent or cyclic JS node references.

Classic tags execute in document order after tree construction; async/defer
parser timing is not fully implemented. Modules, workers, WebSocket, CSSOM,
requestAnimationFrame, capture-phase events, full selectors, form text input,
link navigation and playback are outside this increment. Detached nodes are
retained until navigation or the allocation cap; this is not DOM garbage collection.

Fetch accepts only GET without a body and checks the requested URL against the
page origin. Native transport sends its existing headers, without page-provided
headers or cookies. Successful 2xx bodies produce `ok=true`, `text()` and
`json()`; HTTP failures reject. The ABI does not expose status numbers, response
headers or final URLs. Redirects still follow the existing kernel policy and
cannot yet be checked against the page origin. CSP/CORS and redirect-aware
origin enforcement remain prerequisites for a general-purpose web security
boundary; this realm is an experimental compatibility surface.

## Reproduction

Capture into a new directory (GET only), including the explicit API response:

```sh
python3 tools/capture_page.py http://192.168.178.94:8338/ target/bios-capture --scripts --resource /api/bios/schema
cargo run --locked --example page_probe -- target/bios-capture target/bios.svg 1280 800 --scripts '--expect=#app[aria-busy=false]'
cargo run --locked --example page_probe -- target/bios-capture target/bios-details.svg 1280 800 --scripts --click=#details-button '--expect=#details-drawer.open'
cargo test --locked
```

The probe performs no live network requests: uncaptured script/fetch URLs fail.
This keeps firmware snapshots local and makes runtime regressions reproducible.
