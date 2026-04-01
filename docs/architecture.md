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
- `capture_snapshot()`
- `restore_snapshot()`

### `SimulationSnapshot<State>`

`SimulationSnapshot<State>` is a value type that captures:

- current tick
- elapsed simulated time
- configured seed
- a copy of the simulation state

Snapshots can be restored back into a compatible engine instance. Compatibility is intentionally narrow: restore expects snapshot clock metadata and snapshot seed to match the engine configuration, and expects state tick metadata to match the captured tick.

## Update Flow

For each executed tick:

1. The engine determines the next tick number.
2. The engine derives a per-tick seed from the configured base seed and tick number.
3. A step-local deterministic random source is created from that per-tick seed.
4. An `InputStepContext<Input>` or `StepContext` is created for that tick.
5. `PreUpdate` systems execute in registration order.
6. `Update` systems execute in registration order.
7. `PostUpdate` systems execute in registration order.
8. The clock advances.
9. The state's completed-tick metadata is updated.

No dynamic scheduling occurs in the current implementation. Deterministic ordering depends on the fixed phase order, the registration sequence within each phase, and user code preserving deterministic behavior inside each callback.

The step-local deterministic random source is shared across systems within the tick. Because systems execute in stable registration order, random draws remain reproducible as long as system logic and input sequencing remain unchanged.

When a snapshot is restored, the engine clock and active state are replaced directly from the captured values. Registered systems and their phase assignments are not modified, so continuation after restore uses the same update ordering as uninterrupted execution. The engine does not store a mutable RNG stream in the snapshot; continuing from a restored snapshot remains reproducible because each tick reconstructs its deterministic random source from the configured seed and tick number.

## Stable Ordering

The system container is a simple ordered sequence with explicit phase tags. This is intentional.

The repository does not currently attempt:

- dependency graphs between systems
- dynamic priority resolution
- parallel execution
- automatic rescheduling

Those features can be valuable later, but they would complicate deterministic reasoning at this stage. The current model keeps ordering transparent: phase first, registration order second.

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

- replay event capture
- serialized snapshots
- larger benchmark workloads
- optional scheduling extensions with explicit ordering semantics
