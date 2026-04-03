# xenor-engine

[![CI](https://github.com/XENOr-god/xenor-engine/actions/workflows/ci.yml/badge.svg)](https://github.com/XENOr-god/xenor-engine/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Version: v0.1.0](https://img.shields.io/badge/version-v0.1.0-blue.svg)](https://github.com/XENOr-god/xenor-engine/releases/tag/v0.1.0)

`xenor-engine` is the deterministic engine and replay/snapshot systems
repository in the XENOr stack. It focuses on fixed-timestep execution, explicit
tick progression, replay inspection, and versioned snapshot boundaries, not on
the main XENOr protocol logic itself.

## Status

Active systems repository. The C++ engine has public release tags, and the Rust
workspace is an active determinism and artifact-validation surface. Both remain
deliberately narrow and should be treated as systems infrastructure rather than
the main protocol entry point.

## Why This Repo Exists

This repository exists to isolate reusable deterministic engine concerns:

- fixed-timestep execution
- explicit seed and tick handling
- replay recording and divergence checking
- snapshot capture and restore boundaries
- canonical artifact and parity-prep work in Rust

Keeping that work here prevents `xenor-core` from turning into a generic engine
repo and keeps `xenor-sim` focused on scenario validation instead of lower-level
runtime substrate design.

## Relationship to the XENOr Stack

- `xenor-site` is the canonical public surface and first stop for newcomers
- `xenor-core` is the deterministic execution/core systems layer for XENOr
  logic
- `xenor-sim` is the scenario and validation layer built around `xenor-core`
- `xenor-engine` is the lower-level deterministic engine and replay/snapshot
  substrate
- `xenor-sale` is archived historical research and not part of the active path

If you want protocol logic, start with `xenor-core`. If you want scenario
validation, start with `xenor-sim`. If you want reusable tick/replay/snapshot
infrastructure, start here.

## Quick Start / Local Development

Requirements:

- CMake 3.24 or newer for the C++ engine
- a C++20 compiler
- Rust toolchain if you also want to work in `rust/`

Fastest first run:

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DXENOR_ENGINE_BUILD_TESTS=OFF -DXENOR_ENGINE_BUILD_BENCHMARKS=OFF
cmake --build build --parallel
./build/examples/xenor_engine_resource_pipeline_example
```

Full C++ validation build:

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
ctest --test-dir build --output-on-failure
```

Rust workspace validation:

```bash
cargo test --manifest-path rust/Cargo.toml
```

## Repository Boundaries / Non-goals

- This is not the canonical public website. Use `xenor-site` for that.
- This is not the main XENOr protocol logic layer. Use `xenor-core` for that.
- This is not the primary scenario-validation repo. Use `xenor-sim` for that.
- This is not a sale or launch repository.
- The C++ engine is intentionally narrow and does not try to be a full game
  framework, rendering stack, networking runtime, or ECS.
- The Rust workspace is library-level determinism infrastructure, not a UI or
  deployment surface.

## Engine Scope

The C++ engine in this repository is built around:

- deterministic fixed-timestep execution
- explicit per-tick input sequencing
- stable phase ordering
- snapshot boundaries with adapter-owned payload migration
- replay-oriented inspection through in-memory traces

The Rust workspace under `rust/` is positioned as a determinism and
artifact-validation surface:

- canonical config, scenario, replay, snapshot, and fixture artifacts
- explicit schema/version boundaries
- replay verification and snapshot-assisted resume
- replay inspection and divergence reporting
- parity summaries for future Rust/C++ comparison

## Further Reading

- [docs/architecture.md](docs/architecture.md) — C++ engine architecture and
  ordering model
- [docs/determinism.md](docs/determinism.md) — determinism guarantees and
  snapshot-boundary failure model
- [docs/rust_architecture.md](docs/rust_architecture.md) — Rust deterministic
  architecture and artifact contract
- [`examples/resource_pipeline_example.cpp`](examples/resource_pipeline_example.cpp)
  — end-to-end C++ example

## Related Repositories

- [`xenor-site`](https://github.com/XENOr-god/xenor-site) — canonical public
  surface and first stop for newcomers
- [`xenor-core`](https://github.com/XENOr-god/xenor-core) — deterministic
  execution/core systems layer for XENOr logic
- [`xenor-sim`](https://github.com/XENOr-god/xenor-sim) — scenario and
  validation layer
- [`xenor-sale`](https://github.com/XENOr-god/xenor-sale) — archived
  historical sale prototype

## License

Released under the [MIT License](LICENSE). Contribution guidelines live in
[CONTRIBUTING.md](CONTRIBUTING.md).
