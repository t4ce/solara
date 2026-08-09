# YouTube JavaScript compatibility probe

`youtube-js-probe` is a Linux-only diagnostic executable. It loads a live
YouTube watch page, parses it with RustQJSDom, and executes its classic scripts
in document order in the same retained QuickJS context. It has two deliberately
separate lanes:

- **Strict** is the evidence lane. Its small explicit browser surface has no
  fallback, stops at the first missing contract, and gives every script 500 ms.
- **Scout** is an intentionally unsound census. It runs strict first, then uses
  stateful Proxy phantoms to record reads, writes, calls, construction, and
  feature checks. When QuickJS reports an unresolved global, scout adds only an
  opaque value, creates a fresh runtime, and replays from script zero. Scout has
  a bounded 10-second budget for very large scripts.

Nothing learned by scout is automatically promoted into Solara's browser
surface. A scout success means only that execution continued far enough to
observe more demand; it is never compatibility evidence. A strict success means
only that a script returned against the explicitly installed probe surface.

The probe is intentionally not a media downloader or a second browser engine.
Its small browser bootstrap records touched APIs and stops on the first
JavaScript exception. Strict scripts have a 500 ms QuickJS deadline and scout
scripts have a 10-second deadline, so an untrusted script cannot permanently
occupy the runtime's owning thread.

Run the pinned first target with:

```bash
cargo run --locked --features youtube-js-probe --bin youtube-js-probe
```

Run both the strict evidence lane and the forward scout with:

```bash
cargo run --locked --features youtube-js-probe --bin youtube-js-probe -- \
  --scout
```

Or pass another HTTP(S) watch URL:

```bash
cargo run --locked --features youtube-js-probe --bin youtube-js-probe -- \
  --scout 'https://www.youtube.com/watch?v=nXvnof8fTBc'
```

The live page changes between requests, so byte counts and asset hashes are
expected to move. The useful stable output is the ordered strict result,
`scout_learned_globals`, `scout_api_report`, and the final semantic frontier.
The report includes the first script order that touched each path, occurrence
counts, and a recent-operation tail for diagnosing the frontier.

Before the real Solara input host was installed, the pinned page's strict lane
stopped at `MouseEvent`. The probe now also supplies bounded legacy document
node constructors and real `document.createEvent()` events. In the latest live
observation, strict reached script 8 and stopped at the unsupported `Window`
constructor. Scout executed 36 classic scripts and reached the roughly 10.7 MB
`kevlar_base` bundle at script 37 before its semantic `Hf` error. Those numbers
are observations of a changing page, not fixtures or promised baselines.

This probe fetches HTML and JavaScript only. It does not request, decode, or
store audiovisual content.
