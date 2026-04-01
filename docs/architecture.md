# Architecture

## Intent

`xenor-engine` is structured as a small deterministic execution core. The current design favors explicit control flow, stable ordering, and small public surface area over broad extensibility.

## Core Types

### `SimulationConfig`

`SimulationConfig` defines the fixed tick duration and base deterministic seed for a simulation run. The current implementation validates the tick duration at construction time and rejects zero or negative durations.

### `SimulationClock`

`SimulationClock` is the source of deterministic time progression inside the engine. It tracks:

- current tick as an unsigned integer
- configured tick duration
- elapsed simulated time derived from tick count

The clock advances only through explicit engine calls. It does not consult wall-clock time.

### `SimulationState`

`SimulationState` is a small base class for user-defined state objects. It records the last completed tick and gives the engine a place to store deterministic execution metadata without forcing a larger framework on the state model.

The current design uses inheritance here deliberately and narrowly. It avoids deeper class hierarchies while still giving the engine a consistent state contract.

State types used with `SimulationEngine<State, Input>` must remain copyable because snapshots are captured and restored by value.

### `SystemPhase`

`SystemPhase` defines the fixed execution order used by the engine:

- `PreUpdate`
- `Update`
- `PostUpdate`

The existing `add_system(name, callback)` overload registers systems into `Update`. The phase-aware overload `add_system(SystemPhase, name, callback)` makes ordering intent explicit without adding a larger scheduling system.

### `DeterministicRng`

`DeterministicRng` is a small step-local random source. The engine constructs it from a per-tick seed derived from the configured base seed and the tick number. Systems that need pseudorandom behavior are expected to consume this source through the step context rather than use ambient global randomness.

### `InputSequence<Input>`

`InputSequence<Input>` is a lightweight ordered container for deterministic per-tick inputs. `run_for_sequence()` consumes entries in order, one input per executed tick. The type also exposes `slice()` so callers can resume from a known input offset after snapshot restore.

### `ReplayEventKind`, `ReplayEvent<Input>`, and `ReplayTrace<Input>`

Replay capture is modeled with three small types:

- `ReplayEventKind`
- `ReplayEvent<Input>`
- `ReplayTrace<Input>`

`ReplayTrace<Input>` is an in-memory ordered event sequence. The current event kinds are:

- `TickStarted`
- `InputApplied`
- `SystemExecuted`
- `TickCompleted`
- `SnapshotRestored`

The trace is intended for deterministic inspection and equality comparison in tests. It is not a serialization format and it does not attempt to capture arbitrary user-defined state changes.

### `InputStepContext<Input>` / `StepContext`

`InputStepContext<Input>` is constructed for each executed tick and passed to each registered system. `StepContext` is the no-input alias used by `SimulationEngine<State, NoInput>`. The input-aware form currently contains:

- tick number being executed
- fixed tick duration
- elapsed simulated time at the end of that tick
- configured base seed
- per-tick derived seed
- access to the input value for that tick
- access to the step-local deterministic random source

### `SimulationEngine<State, Input>`

`SimulationEngine<State, Input>` owns:

- the validated simulation configuration
- the simulation clock
- the active simulation state value
- the ordered list of registered systems and their phase assignments

The engine exposes the following execution entry points:

- `step()` for no-input engines
- `step(const Input&)` for input-aware engines
- `run_for_ticks()` for no-input engines
- `run_for_sequence()` for input-aware engines

It also exposes:

- `add_system(name, callback)` for `Update`-phase registration
- `add_system(SystemPhase, name, callback)` for explicit phase selection
- `enable_replay_capture()` / `disable_replay_capture()`
- `clear_replay_trace()`
- `replay_trace()`
- `capture_snapshot()`
- `restore_snapshot()`
- `capture_snapshot_boundary()`
- `restore_snapshot_boundary()`

### `SimulationSnapshot<State>`

`SimulationSnapshot<State>` is a value type that captures:

- current tick
- elapsed simulated time
- configured seed
- a copy of the simulation state

Snapshots can be restored back into a compatible engine instance. Compatibility is intentionally narrow: restore expects snapshot clock metadata and snapshot seed to match the engine configuration, and expects state tick metadata to match the captured tick.

### `SnapshotBoundaryMetadata`, `SnapshotBoundary<Payload>`, and `SnapshotStateAdapter`

Snapshot serialization boundaries are represented with three small pieces:

- `SnapshotBoundaryMetadata`
- `SnapshotBoundary<Payload>`
- `SnapshotStateAdapter`

`SnapshotBoundaryMetadata` carries the engine-owned deterministic metadata required to re-establish a compatible snapshot boundary:

- boundary engine version
- current tick
- elapsed simulated time
- configured seed
- state last-completed-tick metadata

`SnapshotBoundary<Payload>` combines that metadata with a user-defined payload version and payload value. The engine does not infer how arbitrary user state should become a payload. `SnapshotStateAdapter` makes that responsibility explicit through four operations:

- `payload_version() -> snapshot_boundary_payload_version_type`
- `supports_payload_version(version) -> bool`
- `capture(const State&) -> payload_type`
- `restore(const payload_type&, version) -> State`

The engine-level metadata and engine-owned boundary version remain separate from the payload version so future persistence work can keep compatibility checks explicit without forcing a storage format into the core library.

## Update Flow

For each executed tick:

1. The engine determines the next tick number.
2. The engine derives a per-tick seed from the configured base seed and tick number.
3. A step-local deterministic random source is created from that per-tick seed.
4. An `InputStepContext<Input>` or `StepContext` is created for that tick.
5. If replay capture is enabled, a `TickStarted` event is recorded.
6. If an input value is present and replay capture is enabled, an `InputApplied` event is recorded.
7. `PreUpdate` systems execute in registration order.
8. `Update` systems execute in registration order.
9. `PostUpdate` systems execute in registration order.
10. If replay capture is enabled, each system execution records a `SystemExecuted` event before the callback runs.
11. The clock advances.
12. The state's completed-tick metadata is updated.
13. If replay capture is enabled, a `TickCompleted` event is recorded.

No dynamic scheduling occurs in the current implementation. Deterministic ordering depends on the fixed phase order, the registration sequence within each phase, and user code preserving deterministic behavior inside each callback.

The step-local deterministic random source is shared across systems within the tick. Because systems execute in stable registration order, random draws remain reproducible as long as system logic and input sequencing remain unchanged. Replay event order follows the same deterministic structure.

When a snapshot is restored, the engine clock and active state are replaced directly from the captured values. Registered systems and their phase assignments are not modified, so continuation after restore uses the same update ordering as uninterrupted execution. The engine does not store a mutable RNG stream in the snapshot; continuing from a restored snapshot remains reproducible because each tick reconstructs its deterministic random source from the configured seed and tick number. If replay capture is enabled, restore also records a `SnapshotRestored` event.

Snapshot boundary projection sits beside that restore path. `capture_snapshot_boundary()` first captures a normal `SimulationSnapshot<State>`, derives `SnapshotBoundaryMetadata` from it, records the current engine-owned boundary version, and then delegates payload extraction and payload version selection to the provided adapter. `restore_snapshot_boundary()` validates the boundary metadata and engine version against the engine configuration before asking the adapter whether the payload version is supported. Only after those checks does it reconstruct a `SimulationSnapshot<State>` through the adapter and restore it through the normal snapshot path.

The engine treats `SimulationState` metadata as engine-owned. Adapters are responsible for the user-defined state payload, payload version compatibility, and any payload migration behavior. The engine restores `last_completed_tick` from boundary metadata after adapter reconstruction so payload code does not need to duplicate engine metadata handling.

## Stable Ordering

The system container is a simple ordered sequence with explicit phase tags. This is intentional.

The repository does not currently attempt:

- dependency graphs between systems
- dynamic priority resolution
- parallel execution
- automatic rescheduling

Those features can be valuable later, but they would complicate deterministic reasoning at this stage. The current model keeps ordering transparent: phase first, registration order second, optional replay markers on top of that ordering.

## Template Boundary

`SimulationEngine<State, Input>` is header-only because it is templated on the user state type and optional input type. Non-template components such as the clock and configuration live in `src/` and compile into the core library target.

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

- encoded snapshot formats built on the explicit boundary types
- larger benchmark workloads
- optional scheduling extensions with explicit ordering semantics
