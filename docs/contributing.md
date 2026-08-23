# Contributing

Read the accepted-record schema and source notice before editing corpus files.
Install `cargo-llvm-cov` 0.8.7, then run these checks from the repository root:

```bash
cargo install cargo-llvm-cov --version 0.8.7 --locked
CARGO_BUILD_JOBS=4 cargo fmt --check
CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets --locked -- -D warnings
CARGO_BUILD_JOBS=4 cargo test --workspace --all-targets --locked
CARGO_BUILD_JOBS=4 cargo llvm-cov --workspace --all-targets --locked --fail-under-lines 81
cargo run --release --locked --bin toenctl -- corpus check
cargo run --release --locked --bin toenctl -- generate --check
cargo run --release --locked --bin toenctl -- bench smoke --check
```

The enforced workspace line-coverage floor is 81%.

Use original examples, preserve source locators, and do not copy substantial
third-party text. Changes to commands, style, corpus rules, or generated files
must update the English documentation.
