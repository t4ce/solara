# First page script and the CSS lifecycle

The new native [spec-layout integration](spec-layout.md) exposes Blitz's
invalidating Rust mutation API and explicit CSS animation sampling. QuickJS is
not yet bound to that live document: the page-script limitations below still
apply to JavaScript. CSS can now be resolved at a host-supplied time without
requiring a JavaScript animation implementation.

## Current milestone

Solara keeps every `<script>` element in each RustQJSDom artifact, but executes
at most one per artifact. After Parse5 and Lightning CSS have produced and
validated the artifact, the browser host scans the retained scripts in document
order:

1. `importmap`, `module`, JSON, and other non-classic script types remain inert.
2. The first supported classic script is selected. Later scripts remain
   preserved in `extracted.scripts` but are not executed.
3. Inline source is evaluated directly in the retained QuickJS runtime.
4. An external source must have a matching `kind=script` entry in `assetIndex`.
   Solara passes that entry through its browser-owned loader, in the same spirit
   as the existing linked-stylesheet callback, and evaluates the returned text.
   The active probe recognizes only its embedded fixture and performs no network
   fetch.
5. Evaluation has a 500 ms wall-clock limit.

The active loader supplies only a trusted, repository-embedded fixture. This is
not yet a sandbox boundary for arbitrary page code: an isolated page realm or
equivalent protection is required before network scripts may share the retained
runtime without risking parser-private globals.

This is intentionally not full HTML script scheduling. A conforming browser
normally distinguishes parser-blocking, `async`, `defer`, and module scripts and
runs each at its specified point in parsing. Solara currently waits until the
whole document and its initial CSS snapshot are ready, then executes one classic
script as a bounded integration proof.

## How JavaScript and CSS animations are tied together

JavaScript does not directly interpret CSS animations, and CSS does not execute
JavaScript. They meet through shared document/style state and the browser's
frame scheduler:

- JavaScript can change DOM structure, classes, attributes, inline styles, or
  stylesheets. Those mutations invalidate the affected computed style and may
  also invalidate layout, paint, or compositing.
- A CSS transition is started when a new computed value differs from the old
  value for a transitionable property. A CSS keyframe animation becomes active
  when the winning `animation-*` declarations attach it to an element.
- On an animated frame, the browser advances its animation timeline, runs the
  relevant JavaScript callbacks such as `requestAnimationFrame`, resolves style
  changes, performs any required layout and paint, and presents or composites
  the frame. The exact implementation can optimize or reorder internal work,
  but both systems ultimately feed the same frame lifecycle.
- Transform and opacity animations can often be sampled and composited without
  running JavaScript on every frame, once their inputs are known.
- The Web Animations API exposes much of the same animation model to JavaScript,
  allowing scripts to inspect and control animations that otherwise originate
  in CSS.

The current Solara milestone does not yet expose live DOM objects, CSSOM,
`requestAnimationFrame`, timers, an event loop, animation timelines, or style
invalidation to page code. Therefore a selected script can currently change
QuickJS global state, but it cannot make the already-produced `styleIndex`
reflect a class change or advance a CSS animation. The next meaningful boundary
is a mutation bridge that marks style/layout/paint dirty and recomputes the
projection before a scheduled frame.
