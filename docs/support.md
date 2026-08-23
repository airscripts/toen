# Platform Support

| Platform | Architecture | Release Gate |
| --- | --- | --- |
| Ubuntu 24.04 container | x86-64 | Full verification, coverage, packaging. |
| Windows 2025 | x86-64 | Build, tests, generation, manifests. |
| Windows 11 (ARM64) | ARM64 | Build, tests, generation, manifests. |
| macOS 15 (x86-64) | x86-64 | Build, tests, generation, manifests. |
| Ubuntu 24.04 ARM | ARM64 | Build, tests, generation, manifests. |
| macOS 15 (ARM64) | ARM64 | Build, tests, generation, manifests. |

The maintainer toolchain is Rust 1.89 and the repository keeps `Cargo.lock`
committed. Assistant plugin installation additionally requires the relevant
host CLI; Toen itself has no runtime dependency.

Supported release line:

| Toen Version | Status |
| --- | --- |
| 0.1.x | Unreleased first release line; current repository authority. |

See [SUPPORT.md](../SUPPORT.md) for troubleshooting and support channels.
