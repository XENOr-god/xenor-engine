# xenor-engine

`xenor-engine` is a C++20 deterministic simulation engine for fixed-timestep workloads.

The repository is intended as a disciplined foundation for systems, protocol, mechanism, and economic simulation work where explicit state evolution and repeatable execution matter more than feature breadth.

## Project Scope

The current repository provides:

- a small reusable engine core
- explicit fixed-timestep execution
- stable system registration and update ordering
- user-owned simulation state with deterministic tick metadata
- snapshot-friendly state capture
- one runnable deterministic example
- baseline unit tests
- one benchmark target for tick execution throughput

## Design Goals

- deterministic execution from identical inputs
- fixed-timestep progression in integer ticks
- explicit state transitions
- stable and inspectable update ordering
- architecture that leaves room for replay and snapshot work
- baseline benchmarking support
- small dependency surface

## Non-Goals

`xenor-engine` is not:

- a game engine
- a rendering or graphics stack
- a GUI framework
- a networking runtime
- an ECS framework
- a full protocol simulator

The initial implementation is intentionally narrow. It focuses on the engine substrate rather than domain-specific models.

## Architecture Overview

The engine is organized around a small set of core types:

- `SimulationConfig`
  holds the fixed tick duration
- `SimulationClock`
  tracks integer tick progression and derived simulated time
- `SimulationState`
  provides deterministic tick metadata for user-defined state objects
- `StepContext`
  passes tick-local execution data to systems
- `SimulationEngine<State>`
  owns the clock, the active state value, and the ordered system list
- `SimulationSnapshot<State>`
  captures tick, elapsed simulated time, and a copy of the current state

Systems are registered explicitly and execute in registration order. The engine does not perform any dynamic scheduling or wall-clock based stepping.

Additional design notes are documented in [docs/architecture.md](docs/architecture.md) and [docs/determinism.md](docs/determinism.md).

## Determinism Philosophy

The repository treats determinism as a design constraint rather than an optional runtime mode.

The current implementation follows these rules:

- tick duration is configured explicitly and must be positive
- engine time advances in integer ticks only
- the engine never reads wall-clock time
- system execution order is stable and explicit
- snapshots are copies of deterministic state plus clock metadata
- repeated runs with identical initial state and identical system logic should produce identical results

This does not remove all sources of nondeterminism from user code. Callbacks can still introduce nondeterministic behavior if they read wall-clock time, use unstable containers for externally visible ordering, depend on non-repeatable random input, or rely on undefined behavior. The engine is structured to make those hazards visible rather than hide them.

## Current Implementation Status

The repository is a first credible foundation, not a complete framework.

What exists today:

- core fixed-timestep engine library
- deterministic resource-pipeline example
- unit tests for tick progression, ordering, snapshots, and repeatability
- benchmark target for repeated tick execution
- Linux CI for configure, build, and test

What does not exist yet:

- serialization or persistence
- replay logs
- state diffing
- rollback support
- parallel scheduling
- domain-specific simulation models

## Build Instructions

The project uses CMake and requires a C++20 compiler.

Configure:

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
```

Build:

```bash
cmake --build build --parallel
```

The current CMake configuration fetches Catch2 and Google Benchmark during configuration when tests and benchmarks are enabled.

## Test Instructions

```bash
ctest --test-dir build --output-on-failure
```

## Benchmark Instructions

Build the benchmark target as part of the normal build, then run:

```bash
./build/benchmarks/xenor_engine_benchmarks
```

The benchmark is intentionally small. It exists to establish a baseline measurement path rather than provide authoritative performance claims.

## Example Usage

Build and run the example:

```bash
./build/examples/xenor_engine_resource_pipeline_example
```

The example runs a deterministic four-stage resource pipeline and verifies that two identical runs produce the same final state before printing the snapshot summary.

## Minimal API Sketch

```cpp
#include <chrono>
#include "xenor/xenor.hpp"

struct State final : xenor::SimulationState {
  std::uint64_t counter{0};
};

int main() {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<State> engine{xenor::SimulationConfig{1ms}};
  engine.add_system("increment", [](State& state, const xenor::StepContext&) {
    state.counter += 1;
  });

  engine.run_for_ticks(10);
}
```

## Repository Layout

- `include/xenor/`
  public engine headers
- `src/`
  non-template library implementation
- `examples/`
  runnable deterministic example workloads
- `tests/`
  unit tests
- `benchmarks/`
  Google Benchmark targets
- `docs/`
  architecture and determinism notes
- `cmake/`
  project-local CMake modules
- `.github/workflows/`
  CI configuration

## Roadmap

- add explicit replay event capture
- add snapshot serialization boundaries
- add deterministic input stream handling
- add more representative benchmark scenarios
- add state inspection helpers for larger workloads
- evaluate scheduling extensions without weakening ordering guarantees

## Development Notes

- Prefer integer-based state and explicit ordering for simulation logic that must remain reproducible.
- Keep system callbacks small and explicit.
- Treat benchmark results as workload-specific observations, not universal claims.
- Avoid introducing dependencies that obscure state flow or execution order.
