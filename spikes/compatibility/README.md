# CodeAtlas compatibility spike

This isolated binary proves only the dependency combinations required by Phase 00.
It is not the CodeAtlas server.

With Rust 1.98.1 available, rerun the complete spike from this directory:

```sh
cargo run --locked -- probe
COMPAT_ARTIFACT_DIR=artifacts cargo test --locked
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
```

`serve` starts the disposable rmcp status server on stdio. It writes no diagnostics
to stdout. The integration test starts that real binary twice and records independent
modern and legacy JSON-RPC frames when `COMPAT_ARTIFACT_DIR` is set.
