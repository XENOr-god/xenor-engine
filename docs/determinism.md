# Determinism Notes

## What the Engine Guarantees Today

Within the current design, `xenor-engine` guarantees:

- fixed tick duration is validated and explicit
- simulation progression is driven by integer ticks
- system execution order is stable and registration-based
- engine time is derived from tick count rather than wall-clock time
- snapshots capture deterministic clock metadata alongside state

These guarantees are intentionally narrow. They define the engine contract, not the full behavior of arbitrary user code.

## What User Code Must Still Preserve

User-defined systems can still break repeatability if they:

- read wall-clock time
- depend on nondeterministic external input
- use unstable iteration order where order affects visible results
- rely on undefined behavior
- perform floating-point work that is not controlled for the intended platform

The engine does not attempt to hide or compensate for these choices.

## Ordering Discipline

Deterministic workloads often fail because ordering assumptions are implicit.

The current repository avoids implicit ordering by:

- requiring explicit system registration
- executing systems in a stable sequence
- keeping tick advancement centralized in the engine

If future scheduling features are added, they should preserve this clarity rather than replace it with hidden runtime behavior.

## Snapshots and Replay

Snapshot support in the current repository is intentionally simple: a snapshot is a copy of state plus clock position.

This is sufficient to establish:

- a concrete checkpoint boundary
- a repeatability comparison mechanism in tests and examples
- a clear path toward future replay support

Replay logs, serialized state, and input capture are out of scope for the initial repository version.
