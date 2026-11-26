# Locally maintained compatibility patches

These are source patches, not renamed versions of upstream packages. The app's
`src-tauri/Cargo.toml` selects them through `[patch.crates-io]`. The original
versions, licenses, `Cargo.toml.orig`, and `.cargo_vcs_info.json` are retained.
Only Cargo-generated registry markers and nested `Cargo.lock` files are omitted.
The patch integrity check binds the checked-in files, and CI compiles the Linux
desktop paths even though published Linux artifacts remain headless CLI builds.

## Sources

Each source archive is the crates.io distribution at
`https://crates.io/api/v1/crates/<package>/<version>/download`. Verify the complete
archive SHA-256 before comparing or refreshing a patch:

| Package | Archive SHA-256 | License |
| --- | --- | --- |
| glib 0.18.5 | `233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5` | MIT |
| glib-macros 0.18.5 | `0bb0228f477c0900c880fd78c8759b95c7636dbd7842707f49e132378aa2acdc` | MIT |
| gtk3-macros 0.18.2 | `52ff3c5b21f14f0736fed6dcfc0bfb4225ebf5725f3c0209edeec181e4d73e9d` | MIT |
| tauri-plugin-notification 2.8.1 | `14a226bfb8189ec4ab2c1936e66330ad06c44d9f6c54a674338208491dee177f` | MIT OR Apache-2.0 |
| tauri-plugin-single-instance 2.5.5 | `726b6da0dd5bd1344f84f4108a622c07d01923baacb5c803a605189672c5352f` | MIT OR Apache-2.0 |
| serde_with 3.24.0 | `df9adc193c780ef8f159aee8b61e2d5801aaa555e6eb0947fe45530ec506296f` | MIT OR Apache-2.0 |

The original GTK 3 macro archive records an upstream dirty working tree in its
VCS metadata; the crates.io archive checksum above is the precise source identity.

## Unused Chrono Integration

The app's date/time implementation uses Jiff. The `serde_with` patch removes its
unused optional Chrono integration from the resolved graph while preserving all
active serialization implementations and explicitly rejecting attempts to enable
that removed integration. Its precise source changes and removal conditions are
documented in [`serde_with/NETVISION-PATCH.md`](serde_with/NETVISION-PATCH.md).

## GLib iterator backport

`glib-0.18.5/src/variant_iter.rs` applies the two-line fix from
[`gtk-rs/gtk-rs-core#1343`](https://github.com/gtk-rs/gtk-rs-core/pull/1343), merged
as commit `05dff0ee696f9bcd8617cd48c4b812d046d440cb`: the C out-pointer is declared
mutable and passed as `&mut p`, eliminating the write through an immutable Rust
reference described by RUSTSEC-2024-0429. The local regression test
`variant_str_iter_mutable_out_pointer_regression` covers all affected iterator
methods and mixed iteration. CI runs it with optimizations, where the original
undefined behavior could discard the C pointer write.

The original version is deliberately preserved. The backport is independently
checked against the source digest, rather than claiming an upstream GLib 0.20+
upgrade. Remove this patch only when Tauri's complete GTK dependency family uses
the fixed GLib release and this optimized regression test passes on that graph.

## Macro diagnostics

`glib-macros-0.18.5` and `gtk3-macros-0.18.2` remove the retired diagnostic
dependency and use their existing Syn 2 dependency's `Error`, `Result`, and
`to_compile_error` APIs. Parser errors propagate as ordinary results; derives
emit compile-error tokens, and duplicate capture errors preserve both source
positions. The changed files are the normalized manifests, macro entry points,
and the parser/derive helpers formerly importing `proc_macro_error`. Seven new
tests cover invalid and successful closure/variant/template parsing. CI runs
the macro-library tests and compiles their real GTK consumers.
Do not reintroduce `proc-macro-error` or `proc-macro-error2` as a shortcut. Remove
these patches when the GTK family selected by Tauri uses maintained diagnostics
upstream and the same consumer and malformed-input tests pass.

## D-Bus migration

`tauri-plugin-notification-2.8.1/Cargo.toml` selects notify-rust 4.19 and zbus 6.5
and adds futures-util for the async signal stream. `src/desktop.rs` uses the zbus
6 connection, builder, variant and message-stream APIs. A signal subscription is
established before sending tracked notifications. User callbacks run outside the
zbus executor; synchronous bus operations use a separate thread so async Tauri
commands cannot nest that executor. Existing signal tests are retained, with
additional malformed-signal and async-command bridge regressions.

`tauri-plugin-single-instance-2.5.5/Cargo.toml` selects zbus 6.5. Its
`src/platform_impl/linux.rs` migrates to async connection APIs, validates the bus
name and object path before opening the connection, and runs the user callback
off the D-Bus executor. Lock destruction releases the name from a separate thread,
allowing a callback to release its own lock. `tests/dbus_errors.rs` is adapted to
the new builder API; `tests/instance_lock.rs` adds the callback-release regression.
The original unavailable-bus fallback and secondary-instance failure propagation
are preserved and checked using a real `dbus-run-session` in CI.

zbus 6 incorporates the retired standalone zbus_names, zvariant and
zvariant_derive packages into its maintained crate. Remove these two patches
after compatible upstream plugins use zbus 6+ and notify-rust 4.19+ and their
notification and actual session-bus lifecycle tests pass. Do not merely weaken
dependency audit policy.

## Refresh Procedure

1. Download and verify the named official archives. Keep all required license
   notices and upstream source identity files.
2. Compare each local source with its upstream archive. Review every change;
   do not regenerate the integrity manifest around unexpected files.
3. Apply the smallest compatible fix or remove a patch when its upstream
   replacement meets the conditions above.
4. Update the app lockfile, run the source integrity regression tests and strict
   dependency audit, then run the Windows and Linux desktop checks and all four
   headless CLI checks before publishing a release.
