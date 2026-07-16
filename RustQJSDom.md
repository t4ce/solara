# RustQJSDom migration archive

The former standalone workspace checkout was archived here before workspace
restructuring. Solara continues to contain the verified engine revision as the
`vendor/RustQJSDom` git submodule.

## Archive

- File: `RustQJSDom.7z`
- Format: 7-Zip / LZMA2, solid archive
- SHA-256: `cd7ac14b45acf3bd63aa4be6bc1c58a02000b2aff3a2b6998f0059af98afa26c`
- Uncompressed source size: 4,928,085 bytes
- Archived entries: 378 files and 177 directories
- RustQJSDom revision: `874e09c2d96579a16b664661582cd46fbd9a7846`
- Solara integration revision: `c05ec9b54c9e6c1e98c45d0758cf481070a00161`

The archive contains the complete clean standalone checkout, including its
`.git` metadata. Its 1.9 GB Cargo `target/` directory was deleted before the
archive was created and is not present in the archive.

The archive passed `7z t`, and its listing was checked for `.git/HEAD`,
`Cargo.toml`, and `README.md` before the standalone directory was removed.

## Migrated boundary

The preserved revision contains the completed migration scope:

- owned std-enabled QuickJS runtime;
- Parse5 normalized DOM artifact;
- hosted Lightning CSS step directly after Parse5;
- renderer-neutral computed-style table and per-node style references;
- external stylesheet callback owned by Solara;
- HTML, CSS URL, image, `srcset`, media, iframe, script, preload, and favicon
  request discovery;
- trace-only Solara asset URL resolution and request logging;
- resolved favicon metadata retained by Solara;
- no image/media decoding, caching, streaming, upload, or paint wiring;
- the previous Solara CSS experiment was preserved at archive time; it was
  subsequently removed from the active fork in favor of the sole
  RustQJSDom/Lightning CSS path.

Before archival, RustQJSDom's full proof script passed. After pinning the engine,
Solara passed formatting, locked check, 17 tests, and Clippy with warnings denied.
The approved no-author-CSS render digest remained unchanged, and an integration
test proved that authored Lightning CSS reaches the active paint batch.

## Restore

From the Solara repository's parent directory:

```bash
7z x solara/RustQJSDom.7z
git -C RustQJSDom status
git -C RustQJSDom rev-parse HEAD
```

The expected restored revision is
`874e09c2d96579a16b664661582cd46fbd9a7846`. Build output can be regenerated
with Cargo and was intentionally not archived.
