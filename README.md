# xenor-engine

`xenor-engine` is a C++20 deterministic simulation engine for fixed-timestep workloads.

The repository is intended as a disciplined foundation for systems, protocol, mechanism, and economic simulation work where explicit state evolution and repeatable execution matter more than feature breadth.

## Project Scope

The current repository provides:

- a small reusable engine core
- explicit fixed-timestep execution
- explicit seed handling in simulation configuration
- stable system registration and update ordering
- fixed phased system execution
- deterministic per-tick input sequencing
- optional in-memory replay event capture
- user-owned simulation state with deterministic tick metadata
- tick-scoped deterministic random number generation
- snapshot capture and restore
- deterministic restore-and-continue validation
- one runnable deterministic example
- baseline unit tests
- one benchmark target for tick execution throughput

## Design Goals

- deterministic execution from identical initial state, configuration, seed, and input sequence
- fixed-timestep progression in integer ticks
- explicit state transitions
- explicit seed handling with no hidden global randomness
- deterministic per-tick input application
- deterministic execution structure through fixed system phases
- deterministic execution inspection through replay event traces
- stable and inspectable update ordering
- snapshot restore without hidden runtime state
- architecture that supports replay validation and future replay-oriented features
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
  holds the fixed tick duration and base deterministic seed
- `SimulationClock`
  tracks integer tick progression and derived simulated time
- `SimulationState`
  provides deterministic tick metadata for user-defined state objects
- `SystemPhase`
  defines the fixed execution order `PreUpdate`, `Update`, `PostUpdate`
- `DeterministicRng`
  provides a small deterministic random source for step-local use
- `InputSequence<Input>`
  stores explicit per-tick inputs in tick order
- `ReplayEventKind`, `ReplayEvent<Input>`, `ReplayTrace<Input>`
  model deterministic in-memory replay traces
- `InputStepContext<Input>` / `StepContext`
  passes tick-local execution data, seed data, and optional input access to systems
- `SimulationEngine<State, Input>`
  owns the clock, the active state value, and the ordered system list
- `SimulationSnapshot<State>`
  captures tick, elapsed simulated time, the configured seed, and a copy of the current state

Systems are registered explicitly and execute in registration order. The engine does not perform any dynamic scheduling or wall-clock based stepping.
Systems execute in fixed phase order: `PreUpdate`, `Update`, then `PostUpdate`. Within a phase, execution order remains registration-based. Systems added through the existing `add_system(name, callback)` overload default to the `Update` phase.

State types used with `SimulationEngine<State, Input>` must be copyable. Snapshot capture and restore operate on value copies of the active state.

Input-aware engines consume one explicit input value per executed tick. For each tick, the engine derives a step seed from the configured base seed and tick number, then exposes a deterministic random source through the step context.
Replay capture is optional and in-memory. When enabled, the engine records deterministic tick start, input applied, system executed, tick completed, and snapshot restored events into a `ReplayTrace<Input>`.

Additional design notes are documented in [docs/architecture.md](docs/architecture.md) and [docs/determinism.md](docs/determinism.md).

## Determinism Philosophy

The repository treats determinism as a design constraint rather than an optional runtime mode.

The current implementation follows these rules:

- tick duration is configured explicitly and must be positive
- the base deterministic seed is configured explicitly
- engine time advances in integer ticks only
- the engine never reads wall-clock time
- per-tick inputs are applied in explicit sequence order
- phase order is fixed and explicit
- system execution order within each phase is stable and explicit
- each executed tick receives a deterministic random source derived from the configured seed and tick number
- replay event order is deterministic when replay capture is enabled
- snapshots are copies of deterministic state plus clock metadata
- restoring a captured snapshot restores the exact tick, elapsed time, configured seed, and state value
- repeated runs with identical initial state, configuration, seed, inputs, phase registration, and system logic should produce identical results and identical replay traces

This does not remove all sources of nondeterminism from user code. Callbacks can still introduce nondeterministic behavior if they read wall-clock time, use unstable containers for externally visible ordering, depend on nondeterministic external input, bypass the step-local deterministic random source, or rely on undefined behavior. Replay traces reflect those choices; they do not compensate for them.

## Current Implementation Status

The repository is a first credible foundation, not a complete framework.

What exists today:

- core fixed-timestep engine library
- explicit seed handling through `SimulationConfig`
- phased system registration through `SystemPhase` and `add_system(SystemPhase, ...)`
- deterministic per-tick input sequencing through `InputSequence<Input>` and `run_for_sequence()`
- optional replay traces through `enable_replay_capture()` and `replay_trace()`
- snapshot capture and restore
- tick-scoped deterministic random number generation through the step context
- deterministic resource-pipeline example with phased execution, seed reuse, input sequencing, replay-trace summaries, and restore-and-continue validation
- unit tests for tick progression, phase ordering, seed handling, input sequencing, replay traces, snapshot capture and restore, and repeatability
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

The example runs a deterministic four-stage resource pipeline, captures a mid-run snapshot, continues execution, restores the snapshot, and verifies that the replayed continuation matches the uninterrupted run.
It uses explicit `PreUpdate`, `Update`, and `PostUpdate` phases, an explicit per-tick input sequence, a configured seed, and a tick-scoped deterministic random source. It enables replay capture, compares repeated traces for equality, and prints a compact trace summary.

## Minimal API Sketch

```cpp
#include <chrono>
#include "xenor/xenor.hpp"

struct State final : xenor::SimulationState {
  std::uint64_t counter{0};
};

struct TickInput {
  std::uint64_t delta{0};
};

int main() {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<State, TickInput> engine{xenor::SimulationConfig{1ms, 41}};
  engine.enable_replay_capture();
  engine.add_system(xenor::SystemPhase::PreUpdate,
                    "apply_input",
                    [](State& state, const xenor::InputStepContext<TickInput>& context) {
    state.counter += context.input().delta;
  });
  engine.add_system("increment", [](State& state, const xenor::InputStepContext<TickInput>& context) {
    state.counter += context.rng().next_u64() % 2ULL;
  });
  engine.add_system(xenor::SystemPhase::PostUpdate,
                    "finalize",
                    [](State& state, const xenor::InputStepContext<TickInput>&) {
    state.counter += 1;
  });

  const xenor::InputSequence<TickInput> inputs{{{1}, {2}, {3}}};
  engine.run_for_sequence(inputs);
  const auto snapshot = engine.capture_snapshot();
  const auto event_count = engine.replay_trace().events.size();

  engine.restore_snapshot(snapshot);
  static_cast<void>(event_count);
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

- add snapshot serialization boundaries
- add more representative benchmark scenarios
- add state inspection helpers for larger workloads
- evaluate scheduling extensions without weakening ordering guarantees

## Development Notes

- Prefer integer-based state and explicit ordering for simulation logic that must remain reproducible.
- Keep seeds and per-tick inputs explicit at the engine boundary.
- Use phases to separate preparation, state mutation, and derived-state work when that structure clarifies deterministic ordering.
- Use the step-context random source instead of ambient global randomness when repeatability matters.
- Use replay traces for inspection and regression validation, not as a persistence format.
- Keep system callbacks small and explicit.
- Treat benchmark results as workload-specific observations, not universal claims.
- Avoid introducing dependencies that obscure state flow or execution order.
