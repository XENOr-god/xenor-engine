# Determinism Notes

## What the Engine Guarantees Today

Within the current design, `xenor-engine` guarantees:

- fixed tick duration is validated and explicit
- base seed selection is explicit in `SimulationConfig`
- simulation progression is driven by integer ticks
- input-aware engines consume one explicit input value per executed tick
- phase order is fixed as `PreUpdate`, `Update`, `PostUpdate`
- system execution order within each phase is stable and registration-based
- engine time is derived from tick count rather than wall-clock time
- per-tick random state is derived deterministically from the configured seed and tick number
- snapshots capture deterministic clock metadata, the configured seed, and state
- restoring a captured snapshot re-establishes the captured tick, seed contract, and state
- restore-and-continue execution uses the same registered system order as uninterrupted execution
- repeated runs with identical initial state, configuration, seed, system set, phase registration, and input sequence produce identical results

These guarantees are intentionally narrow. They define the engine contract, not the full behavior of arbitrary user code.

## What User Code Must Still Preserve

User-defined systems can still break repeatability if they:

- read wall-clock time
- depend on nondeterministic external input
- consume randomness from sources outside the step context
- use unstable iteration order where order affects visible results
- rely on undefined behavior
- perform floating-point work that is not controlled for the intended platform

The engine does not attempt to hide or compensate for these choices.

## Ordering Discipline

Deterministic workloads often fail because ordering assumptions are implicit.

The current repository avoids implicit ordering by:

- fixing the phase order in the engine
- requiring explicit system registration
- defaulting unqualified registration to `Update`
- applying input values in explicit sequence order
- executing systems in a stable sequence
- keeping tick advancement centralized in the engine

If future scheduling features are added, they should preserve this clarity rather than replace it with hidden runtime behavior.

## Seeds, Randomness, and Inputs

Seed handling is intentionally explicit:

- `SimulationConfig` stores the base seed used by the engine
- each executed tick derives a step seed from the base seed and tick number
- each step context exposes a step-local `DeterministicRng`
- input-aware engines receive one explicit input object per executed tick

This keeps the reproducibility boundary narrow and inspectable. The engine does not maintain a hidden global random source, and it does not infer input timing from wall-clock behavior or asynchronous callbacks.

Because the step-local random source is recreated from the base seed and tick number, snapshot restore does not need to capture mutable RNG state. Re-running the same remaining ticks with the same remaining input sequence reconstructs the same per-tick random streams.

Phase assignments are part of the reproducibility contract. Changing a system from `PreUpdate` to `Update`, or changing registration order within a phase, changes deterministic behavior even when the state, seed, and inputs are unchanged.

## Snapshots and Replay

Snapshot support in the current repository is intentionally simple: a snapshot is a copy of state plus clock position and configured seed.

This is sufficient to establish:

- a concrete checkpoint boundary
- exact restore to a previous deterministic state
- a repeatability comparison mechanism in tests and examples
- a clear path toward future replay support

Restore semantics are intentionally strict:

- the snapshot tick must be representable by the engine clock
- the snapshot elapsed duration must match the elapsed duration implied by the engine's fixed tick duration and tick
- the snapshot seed must match the engine configuration seed
- the snapshot state's completed-tick metadata must match the snapshot tick

The current implementation uses these checks to reject incompatible snapshots rather than attempt implicit correction.

Replay in the current repository means deterministic re-execution under identical initial state, configuration, seed, phase-ordered systems, and inputs, or deterministic continuation after restoring a snapshot and replaying the remaining input sequence. There is no replay log, event stream, or serialized input capture yet.

Replay logs, serialized state, and input capture are out of scope for the initial repository version.
