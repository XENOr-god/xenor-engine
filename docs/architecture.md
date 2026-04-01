# Architecture

## Intent

`xenor-engine` is structured as a small deterministic execution core. The current design favors explicit control flow, stable ordering, and small public surface area over broad extensibility.

## Core Types

### `SimulationConfig`

`SimulationConfig` defines the fixed tick duration for a simulation run. The current implementation validates this value at construction time and rejects zero or negative durations.

### `SimulationClock`

`SimulationClock` is the source of deterministic time progression inside the engine. It tracks:

- current tick as an unsigned integer
- configured tick duration
- elapsed simulated time derived from tick count

The clock advances only through explicit engine calls. It does not consult wall-clock time.

### `SimulationState`

`SimulationState` is a small base class for user-defined state objects. It records the last completed tick and gives the engine a place to store deterministic execution metadata without forcing a larger framework on the state model.

The current design uses inheritance here deliberately and narrowly. It avoids deeper class hierarchies while still giving the engine a consistent state contract.

State types used with `SimulationEngine<State>` must remain copyable because snapshots are captured and restored by value.

### `StepContext`

`StepContext` is constructed for each executed tick and passed to each registered system. It currently contains:

- tick number being executed
- fixed tick duration
- elapsed simulated time at the end of that tick

### `SimulationEngine<State>`

`SimulationEngine<State>` owns:

- the validated simulation configuration
- the simulation clock
- the active simulation state value
- the ordered list of registered systems

The engine exposes two primary execution entry points:

- `step()`
- `run_for_ticks()`

It also exposes:

- `capture_snapshot()`
- `restore_snapshot()`

### `SimulationSnapshot<State>`

`SimulationSnapshot<State>` is a value type that captures:

- current tick
- elapsed simulated time
- a copy of the simulation state

Snapshots can be restored back into a compatible engine instance. Compatibility is intentionally narrow: restore expects snapshot clock metadata to match the engine configuration and expects state tick metadata to match the captured tick.

## Update Flow

For each call to `step()`:

1. The engine determines the next tick number.
2. A `StepContext` is created for that tick.
3. Systems execute in registration order.
4. The clock advances.
5. The state's completed-tick metadata is updated.

No dynamic scheduling occurs in the current implementation. Deterministic ordering depends on the registration sequence and on user code preserving deterministic behavior inside each callback.

When a snapshot is restored, the engine clock and active state are replaced directly from the captured values. Registered systems are not modified, so continuation after restore uses the same update ordering as uninterrupted execution.

## Stable Ordering

The system container is a simple ordered sequence. This is intentional.

The repository does not currently attempt:

- dependency graphs between systems
- dynamic priority resolution
- parallel execution
- automatic rescheduling

Those features can be valuable later, but they would complicate deterministic reasoning at this stage.

## Template Boundary

`SimulationEngine<State>` is header-only because it is templated on the user state type. Non-template components such as the clock and configuration live in `src/` and compile into the core library target.

This split keeps the public API generic without turning the entire repository into a header-only implementation.

## Performance Posture

The implementation is intended to be benchmarkable, not prematurely specialized.

Current choices that support later performance work:

- fixed-timestep execution
- integer tick progression
- explicit state ownership
- no reflection or runtime metadata system
- no hidden scheduler behavior

The benchmark target exists to make changes measurable from the beginning.

## Future Extension Areas

Natural next steps include:

- replay event capture
- serialized snapshots
- deterministic input stream handling
- larger benchmark workloads
- optional scheduling extensions with explicit ordering semantics
