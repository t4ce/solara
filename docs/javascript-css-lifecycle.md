# JavaScript and CSS lifecycle

Native HTTP(S) navigation keeps the parser separate from page JavaScript.
Parse5 constructs the initial artifact, and SpecLayout imports it into Blitz.
Stylo resolves original CSS, Taffy lays out boxes, and Parley shapes text.
`styleIndex` remains a diagnostic parser snapshot.

A new PageRuntime owns an isolated QuickJS context for the page. It builds DOM
bindings whose identities map to the retained Blitz nodes. Classic scripts run
in document order after tree construction. External sources and GET fetches
are polled by the browser; Promise jobs and timer callbacks run in bounded
slices. See [BIOS bring-up](bios-bringup.md) for the precise API and limits.

DOM methods update the JS tree and append a mutation journal. Once callbacks
and jobs finish, one JS/Rust handoff transfers the batch. Rust validates its
node references, applies it through Blitz's DocumentMutator, and the native
loop resolves layout and paints the resulting document. Reflow preserves node
identities, stylesheets, fonts and glyph caches. Navigation drops the old realm
and its pending request handles. Errors leave the last rendered document
available and appear in the navigator.

JavaScript and CSS meet through this document state. A class or attribute
change invalidates the cascade; new computed values can affect layout and CSS
animation state. JavaScript does not interpret CSS declarations, and CSS does
not execute JavaScript. The next frame must resolve the changed state before
publishing geometry.

SpecLayout accepts a host-supplied CSS animation time, but native navigation
still samples it at zero. CSSOM, requestAnimationFrame, Web Animations and a
continuous animation timeline are not implemented. Parser-blocking, async/defer
scheduling, modules, full event propagation and browser security policies also
remain separate work. The current DOM bridge is not a claim of Vue hydration
or full browser compatibility.

The original embedded host corpus remains available. Its trusted first-script
proof uses the parser context, executes only one classic script with a 500 ms
deadline, and demonstrates QuickJS state changes. It is separate from the native
page realm, whose evaluations and Promise slices have a two-second deadline.
