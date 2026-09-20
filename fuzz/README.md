# CodeAtlas fuzz harnesses

These harnesses are intentionally outside the production Cargo workspace. They use
`cargo-fuzz` plus a separate nightly toolchain and never change the pinned stable
toolchain used to build CodeAtlas.

From the repository root, after installing an approved nightly and `cargo-fuzz`:

```text
cargo +nightly fuzz build
cargo +nightly fuzz run parser_extraction -- -max_total_time=300 -timeout=10
cargo +nightly fuzz run boundary_decoders -- -max_total_time=300 -timeout=10
cargo +nightly fuzz run tool_arguments -- -max_total_time=300 -timeout=10
```

Retain minimized crashing inputs under the matching `fuzz/corpus/<target>` directory
and record toolchain, duration, corpus size and crash count. A clean short run is only
bug-finding evidence, not a proof of memory safety or parser isolation.
The committed `fuzz/Cargo.lock` keeps the development-only graph reproducible.
