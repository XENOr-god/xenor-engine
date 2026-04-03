# xenor-engine

[![CI](https://github.com/XENOr-god/xenor-engine/actions/workflows/ci.yml/badge.svg)](https://github.com/XENOr-god/xenor-engine/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Version: v0.1.0](https://img.shields.io/badge/version-v0.1.0-blue.svg)](https://github.com/XENOr-god/xenor-engine/releases/tag/v0.1.0)

`xenor-engine` is a C++20 deterministic fixed-timestep simulation core for workloads that need repeatable state evolution, explicit tick progression, and replay-oriented validation.

It is intentionally narrow. The repository provides a small engine substrate for simulation work where deterministic behavior matters more than feature breadth.

## What It Is

- a deterministic simulation core with integer tick progression
- explicit fixed-timestep execution through `SimulationConfig` and `SimulationClock`
- explicit seed handling, per-tick input sequencing, and fixed phase ordering
- snapshot capture and restore with version-aware snapshot boundaries
- replay-oriented inspection through in-memory trace capture

## What It Is Not

- a persistence layer
- a serializer framework
- a built-in JSON, binary, or text snapshot format
- an engine-owned payload migration system
- a game framework, rendering stack, networking runtime, or ECS

## Getting Started

Requirements:

- CMake 3.24 or newer
- a C++20 compiler

Fastest first run:

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DXENOR_ENGINE_BUILD_TESTS=OFF -DXENOR_ENGINE_BUILD_BENCHMARKS=OFF
cmake --build build --parallel
./build/examples/xenor_engine_resource_pipeline_example
```

This builds the library and the example without test or benchmark dependencies. The example exits with a non-zero status if its deterministic checks fail.

Full validation build:

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
ctest --test-dir build --output-on-failure
```

When tests or benchmarks are enabled, the configure step fetches Catch2 and Google Benchmark.

## First Example To Run

The best first executable is [`examples/resource_pipeline_example.cpp`](examples/resource_pipeline_example.cpp).

It demonstrates:

- fixed phase ordering through `PreUpdate`, `Update`, and `PostUpdate`
- explicit per-tick input sequencing
- deterministic seed handling and tick-scoped RNG use
- replay trace comparison across repeated runs
- snapshot capture, restore, and restore-and-continue validation
- version-aware snapshot boundaries with adapter-owned payload migration

Run it with:

```bash
./build/examples/xenor_engine_resource_pipeline_example
```

If you want the smallest possible control flow first, read the minimal example below before reading the resource-pipeline example.

## Smallest Example

```cpp
#include <chrono>
#include <cstdint>

#include "xenor/xenor.hpp"

struct CounterState final : xenor::SimulationState {
  std::uint64_t value{0};
};

int main() {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms, 41}};
  engine.add_system("increment", [](CounterState& state, const xenor::StepContext&) {
    ++state.value;
  });

  engine.run_for_ticks(4);
  const auto snapshot = engine.capture_snapshot();

  return snapshot.state.value == 4 ? 0 : 1;
}
```

This is the smallest useful flow:

- define a copyable state type derived from `SimulationState`
- construct `SimulationEngine<State>` with an explicit fixed timestep and seed
- register one or more systems
- step or run the engine for a known number of ticks
- capture a snapshot when you need an exact deterministic checkpoint

Use `SimulationEngine<State, Input>` and `run_for_sequence()` when explicit per-tick inputs are part of the deterministic contract.

## Snapshot Boundaries

Snapshot boundaries exist to separate engine-owned deterministic metadata from user-owned state payload handling.

- `SnapshotBoundaryMetadata` is engine-owned
  It carries `engine_version`, tick, elapsed simulated time, seed, and authoritative state metadata such as `last_completed_tick`.
- `SnapshotBoundary<Payload>` is user-owned at the payload layer
  It carries the adapter-defined payload value and `payload_version`.
- `SnapshotStateAdapter` is responsible for payload capture, payload restore, payload version support, and any payload migration behavior.

The engine enforces:

- engine-version compatibility
- deterministic metadata and configuration compatibility
- strict restore ordering
- authoritative re-application of engine-owned metadata

The engine does not:

- serialize payloads
- choose a persistence format
- persist snapshots to disk
- migrate payloads on behalf of user code
- provide hidden compatibility fallbacks

The public snapshot-boundary API lives in [`include/xenor/snapshot_boundary.hpp`](include/xenor/snapshot_boundary.hpp).

## Core Design Goals

- deterministic execution from identical initial state, configuration, seed, inputs, and system ordering
- fixed-timestep progression in integer ticks
- explicit state evolution and stable update ordering
- replay-oriented inspection through in-memory traces
- snapshot restore without hidden runtime state
- explicit snapshot projection boundaries without engine-owned serialization
- small public surface area and readable implementation

## Current Status

`xenor-engine` v0.1.0 is a narrow but usable foundation.

Implemented today:

- fixed-timestep engine core
- explicit seed handling and per-tick input sequencing
- fixed phase ordering
- optional in-memory replay traces
- snapshot capture and restore
- version-aware snapshot boundaries
- adapter-owned payload restore and migration
- baseline tests, one example, one benchmark target, and CI

## Out Of Scope

The current repository does not provide:

- file I/O persistence
- built-in snapshot encoding formats
- serializer infrastructure
- engine-owned payload migration logic
- rollback execution
- parallel scheduling
- broad game-framework features

## Further Reading

- [docs/architecture.md](docs/architecture.md)
  Core types, update flow, snapshot-boundary structure, and ordering model.
- [docs/determinism.md](docs/determinism.md)
  Determinism guarantees, restore ordering, snapshot-boundary failure model, and adapter responsibilities.
- [docs/rust_architecture.md](docs/rust_architecture.md)
  Determinism-first Rust-oriented engine architecture, subsystem boundaries, replay model, and roadmap toward native and web bindings.
- [`include/xenor/snapshot_boundary.hpp`](include/xenor/snapshot_boundary.hpp)
  Public boundary types, error codes, and adapter contract details.
- [`examples/resource_pipeline_example.cpp`](examples/resource_pipeline_example.cpp)
  The current end-to-end example for first-time users.

## Repository Layout

- `include/xenor/`
  public engine headers
- `src/`
  non-template library implementation
- `examples/`
  runnable example workloads
- `tests/`
  unit tests
- `benchmarks/`
  Google Benchmark target
- `docs/`
  architecture and determinism notes
- `rust/`
  Rust-oriented deterministic core workspace with grouped phase scheduling,
  explicit snapshot cadence policy, append-only replay traces, deterministic
  replay/snapshot artifacts, canonical golden fixtures, parity summaries,
  replay inspection helpers, fast-forward replay resume, and divergence
  checking for future native/web binding integration

## Rust Deterministic Core

The Rust workspace under `rust/` is now positioned as a determinism and
artifact-validation surface, not a replacement for the legacy C++ runtime.

- typed simulation config is part of deterministic identity:
  initial state, snapshot policy, validation policy, and deterministic limits
  are exported as canonical config artifacts with explicit payload schema and
  digest
- authoritative collections use deterministic wrappers:
  `DeterministicMap` and `DeterministicList` enforce stable lookup and stable
  iteration so domain growth does not accidentally depend on hash
  randomization
- authoritative state is split from transient tick data and now carries a
  deterministic entity container with stable entity ids and insertion order
- replay, snapshot, and golden fixture exports use canonical text encoding with
  fixed headers, fixed field order, hex-escaped free-form strings, and fail-fast
  decode on duplicate, missing, reordered, or trailing malformed fields
- artifact schema versions stay separate from config payload, command payload,
  and snapshot payload schema versions
- validation policy is explicit:
  `TickBoundary` records `BeforeTickBegin` and `AfterFinalize`,
  `EveryPhase` additionally records `AfterInputApplied` and
  `AfterSimulationGroup`
- replay inspection helpers expose deterministic per-tick summaries with phase
  markers, validation checkpoints, checksum, state digest progression, and
  snapshot presence for debugging mismatches
- scenario artifacts canonically encode `config + base_seed + ordered frames`
  as a reusable deterministic execution contract for fixtures and parity prep
- golden fixtures now bundle config artifact, optional scenario artifact,
  replay artifact, optional snapshot artifact, and a versioned summary for
  future Rust/C++ parity validation
- parity summaries compare config schema/digest, base seed, final tick, final
  checksum, replay digest, optional snapshot digest, and optional scenario
  digest
- state mutation is phase-scoped:
  authoritative state mutates only through `TickContext` during fixed pipeline
  execution; host-facing API surfaces plain artifacts, summaries, and DTO-like
  interop bundles instead of engine internals
- the API surface is interop-ready for native and WASM boundaries:
  callers can build config/scenario contracts, execute scenarios, import/export
  config/scenario/replay/snapshot/fixture artifacts, verify them, and obtain
  digest-oriented summaries without binding to internal scheduler or state
  machinery

## Development Notes

- Keep seeds and per-tick inputs explicit at the engine boundary.
- Use fixed phases to make ordering visible instead of implicit.
- Use `DeterministicMap` / `DeterministicList` for authoritative collections;
  do not introduce hash-randomized structures into deterministic state or
  artifact contracts.
- Prefer the step-context RNG over ambient global randomness.
- Keep scheduler group order explicit: `PreInput -> Input -> Simulation -> PostSimulation -> Finalize`.
- Capture snapshots only through an explicit cadence policy; do not make them implicit side effects.
- Treat config, scenario, replay, snapshot, and fixture artifacts as versioned compatibility boundaries, not ad hoc dumps.
- Keep artifact schema versions distinct from config/command/snapshot payload schema versions; never guess payload contracts during decode.
- Treat deterministic config as part of replay identity, not a convenience default hidden behind seed + inputs.
- Keep validation checkpoints explicit in the pipeline; do not hide invariant enforcement behind ad hoc debug code.
- Keep authoritative mutation inside phase execution through `TickContext`; do
  not mutate state from host-facing helper layers.
- Keep snapshot payload conversion and payload migration in adapters.
- Do not treat `last_completed_tick` as payload-owned state.
- Use append-only replay traces and divergence checks for inspection and regression validation, not as a persistence format.

## License

Released under the [MIT License](LICENSE).
