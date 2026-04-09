# xenor-engine

[![CI](https://github.com/XENOr-god/xenor-engine/actions/workflows/ci.yml/badge.svg)](https://github.com/XENOr-god/xenor-engine/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Version: v0.1.0](https://img.shields.io/badge/version-v0.1.0-blue.svg)](https://github.com/XENOr-god/xenor-engine/releases/tag/v0.1.0)

`xenor-engine` is the canonical deterministic substrate in the XENOr stack. It
owns fixed-timestep execution, explicit tick progression, replay inspection,
snapshot boundaries, and execution-model stability. It is not the main XENOr
protocol logic layer, and it is not the experimental native lab.

## Status

Active systems repository. The C++ engine has public release tags, and the Rust
workspace is an active determinism and artifact-validation surface that is not
published separately. Both remain deliberately narrow and should be treated as
systems infrastructure rather than the main protocol entry point.

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

## What This Is

`xenor-engine` is the canonical deterministic substrate for the public XENOr
stack.

It is where XENOr should make stable claims about:

- fixed-step execution boundaries
- replay and divergence semantics
- snapshot capture and restore boundaries
- stable artifact contracts that other public surfaces can reference

## What This Is Not

`xenor-engine` is not:

- the canonical public surface
- the core logic layer
- the validation layer
- the experimental native lab
- sale or launch infrastructure

## Relationship to xenor-native

`xenor-native` is an experimental native lab. It can explore kernels, ABI
boundaries, checksum paths, verification harnesses, and benchmarking ideas
before they are mature.

`xenor-engine` is where mature substrate work belongs once it is stable enough
to become canonical. The relationship is intentionally one-way:

- incubation can happen in `xenor-native`
- canonical substrate ownership stays in `xenor-engine`

## Relationship to the XENOr Stack

- `xenor-site` is the canonical public surface and first stop for newcomers
- `xenor-core` is the core logic layer for XENOr
- `xenor-sim` is the validation layer built around `xenor-core`
- `xenor-engine` is the canonical deterministic substrate
- `xenor-native` is the experimental native lab
- `xenor-sale` is archived historical work and not part of the active path

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

Settlement vertical-slice demo:

```bash
cargo run --manifest-path rust/Cargo.toml --example settlement_site_demo
```

That command executes the deterministic settlement economy slice, exports a
generated TypeScript contract into `../xenor-web/app/lib/generated/`, and feeds
the `/simulation` page in `xenor-site` with real replay/snapshot/fixture-backed
output.

## Repository Boundaries / Non-goals

- This is not the canonical public website. Use `xenor-site` for that.
- This is not the main XENOr core logic layer. Use `xenor-core` for that.
- This is not the primary validation repo. Use `xenor-sim` for that.
- This is not the experimental native lab. Use `xenor-native` for incubation
  work that has not yet earned canonical substrate status.
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
- a small but real settlement economy slice that proves the stack can run an
  end-to-end deterministic scenario instead of only foundation tests

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
- [`xenor-core`](https://github.com/XENOr-god/xenor-core) — core logic layer
- [`xenor-sim`](https://github.com/XENOr-god/xenor-sim) — validation layer
- [`xenor-native`](https://github.com/XENOr-god/xenor-native) — experimental
  native lab for low-level incubation work
- [`xenor-sale`](https://github.com/XENOr-god/xenor-sale) — archived
  historical work

## Contributing

Contribution guidance lives in [CONTRIBUTING.md](CONTRIBUTING.md). Use issues
or pull requests directly for scoped engine changes.

## License

Released under the [MIT License](LICENSE).
