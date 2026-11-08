# Contributing

Framecap is experimental. Before proposing a change, consult the
[documentation index](INDEX.md) and [open work](TODO.md). Describe the
environment and expected and observed behavior when reporting a problem; remove
personal paths and other sensitive information from logs before sharing them.

## Development

Linux, Rust, Cargo, a Vulkan loader and driver are needed. Build and run tests:

```sh
cargo fmt --check
cargo build --locked
cargo test --locked
```

The `vkcube` integration test also needs `vkcube`, `timeout`, a working display,
and the built shared library. It can skip itself when prerequisites are
unavailable; a passing headless run is not evidence of visual compatibility.
See [installation](documentation/INSTALL.md) and
[validation](documentation/VALIDATION.md).

Keep changes focused. Add tests for behavior changes and update the applicable
technical reference, configuration example, and [changelog](CHANGELOG.md).
Do not include local logs, generated build outputs, or machine-specific data in
contributions.
