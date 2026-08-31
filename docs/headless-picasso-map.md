# Solara headless-to-Picasso map

Solara is being prepared as a browser-owned, headless scene compiler.  It
parses the document, interprets the supported CSS cascade, owns QuickJS and
browser state, and publishes only draw-oriented facts.  It does not own a UI4
frame, a surface lease, GPU addresses, or scanout lifetime.

The immediate path is:

```text
HTML + CSS + opt-in script step
             |
             v
Solara HeadlessDocument
  DOM projection -> layout -> ordered paint batch -> retained PicassoScene
             |
             v
Picasso compatibility lowerer (temporary)
             |
             v
UI4 Frame -> SURFLIVE -> scanout
```

`HeadlessDocument` is deliberately above the UI4 adapter.  It can parse and
compile a scene without opening a frame; `trueos_app.rs` is now only the
current presenter of that scene.  On TRUEOS this route already has no desktop
WGPU dependency.  The hosted `headless-picasso` Cargo feature exposes the same
compiler seam for tests and future non-windowed tools; it does not yet remove
the desktop renderer dependencies from a normal Linux package.

## What is true today

- RustQJSDom owns Parse5 and Lightning CSS.  The resulting `DomArtifact` and
  `styleIndex` are the only HTML/CSS input Solara projects into layout.
- Solara's Blueprint builds a retained `PicassoScene` from shapes, text
  lookups, and image facts before it touches UI4.
- The current lowerer is a V0 compatibility adapter.  It still becomes UI4
  sprite quads and one retained FontKernel canvas; it is not the native
  Picasso transaction ABI or the final ordered GPU walker.
- The `TRUEOS-Picasso` repository's redb code is an append-only hosted glTF
  import/revision store.  It is useful for immutable asset ingestion and
  revisioned resource provenance, but it is not the Blueprint's mutable DOM
  scene store and should not be linked directly into the guest.
- The correct immediate data seam is
  `trueos_helio_runtime::scene_db`: retained, generation-bearing CPU rows
  whose consumers only read published data.  Its role is structural; this
  does not make Solara a Helio renderer or give the guest Helio GPU access.

That distinction keeps the semantic authority in Solara and puts durable
asset revisions below the scene compiler, rather than turning redb records
into a second DOM.

## Feature steps

| Step | Build/runtime selection | Current behavior | Deliberate limit |
| --- | --- | --- | --- |
| Static CSS scene | default | Parse5 + Lightning CSS -> Solara layout -> retained scene | Page scripts remain inert. |
| Headless scene seam | `headless-picasso` | Compiles the DOM projection without a frame or GPU handle | Current `PicassoScene` is still the V0 compatibility schema. |
| Sandboxed scene patches | `sandboxed-scene-js` plus `<html data-solara-feature-step="sandboxed-scene-js">` | Runs bounded inline classic scripts after CSS parsing, then recompiles one scene | No live browser `document`, external scripts, fetch, filesystem, UI4 lease, or GPU capability. |
| Native Picasso snapshot | future | Stable DOM/fragment identities and an atomic multi-table snapshot | Requires the V1 contract/transport, not a direct UI4 extension. |

The script step is intentionally narrow.  Its only Solara-provided capability
is a frozen `__solara.scenePatch(...)` function.  It accepts exactly one of:

```js
__solara.scenePatch({ op: "set-primary-heading", text: "new heading" });
__solara.scenePatch({ op: "set-first-plain-text", text: "new text" });
```

Scripts are inline classic JavaScript only (missing `type`, `text/javascript`,
`application/javascript`, or `text/ecmascript`); module and data scripts are
rejected.  They must opt in at the document `<html>` element, and are limited
to four scripts, 16 KiB per script, 32 patches, 4 KiB per patch text, and 16 ms
per evaluation by default.  They run in a distinct QuickJS context
with an 8 MiB memory limit and 512 KiB stack limit, so the parser/cascade
runtime's private globals are not shared.  A failed script batch applies no
collected patches, so the previous static CSS projection remains coherent.
This is a capability-test for the mutation boundary, not a claim of DOM API
compatibility: the typed `DomArtifact` remains immutable and no JavaScript
mutation is falsely presented as a live DOM mutation.

## The native Picasso map

The durable target is the repository-wide
[Solara–Picasso contract](../../../../TRUEOS/tools/docs/PICASSO_DOM_SCENEDB_CONTRACT.md):

1. Solara assigns stable `DomRef` and `FragmentKey` values, then emits visual
   fragments only—spatial, clip, group, primitive, hit, resource, and payload
   rows.  No DOM tree is copied into Picasso.
2. One Solara scene compiler coordinates an atomic, immutable publication with
   a common commit epoch.  JavaScript, image decode, video, and font completion
   only enqueue changes for that writer.
3. Picasso derives transforms, clips, damage, bins, and GPU work from the
   published snapshot.  Those products remain disposable renderer state.
4. A pointer-free guest transaction crosses to the host for validation,
   resource pinning, a render ticket, a checked UI4 target lease, exact
   release, and SURFLIVE publication.

The next implementation slice should therefore land the V1 identity and
multi-table publication coordinator in the TRUEOS runtime, then change
`HeadlessDocument::rebuild_scene` from its V0 ordered stream to a real
Solara-owned snapshot compiler.  Replacing the temporary FontCanvas/sprite
lowerer belongs after that boundary, not before it.

## Checks

Run the normal Solara tests plus the first execution feature step:

```bash
cargo test --locked
cargo test --locked --features sandboxed-scene-js
cargo fmt --all -- --check
```
