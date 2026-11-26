# NetVision serde_with patch

This is `serde_with` 3.24.0 from the official crates.io crate archive:

- Source: https://crates.io/api/v1/crates/serde_with/3.24.0/download
- Upstream: https://github.com/jonasbb/serde_with
- Archive SHA-256: `df9adc193c780ef8f159aee8b61e2d5801aaa555e6eb0947fe45530ec506296f`
- License: MIT OR Apache-2.0; both upstream license files are included.

The upstream package version and all active serialization implementations are
unchanged. `Cargo.toml.orig` is retained as the unmodified upstream manifest.
Only the normalized `Cargo.toml` and the chrono module exports in `src/lib.rs`
are patched:

1. Remove the optional `chrono_0_4` dependency and its weak `alloc`/`std` feature
   edges. NetVision does not enable this integration. Cargo otherwise includes
   the unused dependency in the lockfile, despite it being absent from every
   actual build graph after NetVision's migration to Jiff.
2. Retain the declared `chrono` and `chrono_0_4` feature names so existing source
   and documentation cfg expressions remain known to the compiler. Enabling
   either now produces an explicit compile error instead of silently restoring
   the unmaintained dependency or pretending that the integration still works.
   The chrono-specific implementation file is preserved as upstream source but
   is no longer exported or compiled.

This does not suppress RustSec advisories, alter advisory databases, forge a
package version, or manually remove resolved dependencies from Cargo.lock.
RUSTSEC-2026-0543 reports every chrono version as unmaintained. No upstream
upgrade removes that integration, including serde_with 3.25.0 checked on
2026-11-27. Revisit this patch when serde_with offers a maintained alternative
without the optional chrono edge.

The registry's generated `.cargo-ok` marker and the crate's private Cargo.lock
are not copied. Cargo resolves this path dependency through NetVision's root
lockfile. Upstream tests and documentation are otherwise preserved; chrono
feature tests are intentionally unsupported by this restricted patch.
