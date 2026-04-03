# Rust-Oriented Deterministic Engine Architecture

## Goals

- deterministic-first execution from `typed config + seed + ordered input frames`
- replayability as a first-class debugging and validation surface
- explicit subsystem boundaries that remain testable in isolation
- clean separation between engine core and external consumers such as
  `xenor-native` and `xenor-site`
- architecture that can target native bindings and WASM without hidden state

## Non-goals

- CI/CD design
- frontend or UI concerns
- deployment concerns
- broad framework features unrelated to deterministic simulation

## Architecture

### High-Level Overview

`xenor-engine` should center on one narrow contract:

`typed config + seed + ordered input frames + phase ordering + schema versions -> identical snapshots, checksums, replay results, and parity summaries`

The engine is best treated as a deterministic execution kernel composed of:

- `core`
  Shared deterministic types, error model, seed/tick helpers, and checksum
  utilities. This layer does not depend on runtime orchestration.
- `config`
  Typed simulation configuration, canonical config artifacts, and config
  identity digests that remain separate from replay input and runtime state.
- `state`
  Authoritative simulation state and snapshot projection contract.
- `input`
  Ordered per-tick command frames. No implicit events, no hidden input timing.
- `rng`
  Seeded deterministic randomness only. No ambient global RNG.
- `phases`
  Stateful or stateless systems that mutate simulation state through explicit
  tick context.
- `scheduler`
  Fixed, inspectable ordering across phases/systems.
- `validation`
  Explicit invariant checkpoints and validation policy for deterministic
  state-authority enforcement.
- `replay`
  Deterministic recording of inputs, phase execution, snapshots, and checksums.
- `serialization`
  Deterministic config, command, and snapshot payload encoding with explicit
  payload schema versioning.
- `scenario`
  Deterministic scenario contract over config, seed, ordered frames, and
  optional expected parity summary.
- `fixture`
  Canonical golden fixture generation, import/export, and verification for
  replay-based regression and future parity work.
- `parity`
  Small comparison surface for digest-oriented Rust/C++ parity preparation.
- `engine`
  Main loop orchestration. This is where seed, input, phases, replay, and
  snapshots meet.
- `api`
  Safe, stable surface for external callers.
- `bindings`
  Thin adapters for native, web, or FFI-facing layers. Bindings should be leaf
  modules, never owners of engine truth.

### Determinism Principle Per Layer

- `core`
  No wall-clock, no I/O, no ambient mutable globals.
- `input`
  Inputs are explicit frames with explicit tick identity.
- `rng`
  Every random draw comes from a reproducible seed path.
- `phases`
  Systems mutate only through the provided context. Hidden state is forbidden.
- `scheduler`
  Ordering is visible and stable.
- `replay`
  Logging is append-only and derived from explicit engine events.
- `serialization`
  Snapshot bytes are based on semantic field order, never memory layout.
- `api` / `bindings`
  Consumers observe deterministic state; they do not own or mutate hidden
  engine internals.

## Modules

### Proposed Rust Module Tree

```text
rust/
  Cargo.toml
  src/
    lib.rs
    core/
    config/
    state/
    input/
    rng/
    validation/
    replay/
    persistence/
    serialization/
    scenario/
    fixture/
    parity/
    phases/
    scheduler/
    engine/
    api/
    bindings/
```

### Module Responsibilities

- `core`
  Seed/tick/checksum helpers, deterministic errors, shared scalar types.
- `config`
  `ConfigArtifact`, config identity digesting, and typed runtime/init contract.
- `state`
  `SimulationState` contract and canonical snapshot boundary inside the Rust
  architecture.
- `input`
  `Command` trait and `InputFrame`.
- `rng`
  `Rng` trait and seeded deterministic implementations.
- `replay`
  `ReplayLog` trait, append-only tick records, snapshot metadata, and
  divergence reporting.
- `persistence`
  Versioned replay and snapshot artifact codecs, summary generation, and
  deterministic replay execution helpers.
- `serialization`
  `Serializer` trait and deterministic schema-oriented payload codecs.
- `fixture`
  Golden fixture bundle surface with versioned summary and deterministic
  import/export helpers.
- `parity`
  Digest-oriented summary structs and mismatch reporting for future cross-engine
  comparison.
- `phases`
  `Phase` trait and `TickContext`.
- `scheduler`
  `Scheduler` trait and fixed-order scheduler implementation.
- `validation`
  `StateValidator`, `ValidationPolicy`, explicit checkpoints, and deterministic
  validation summaries recorded into replay.
- `engine`
  `Engine` trait and concrete deterministic engine loop.
- `scenario`
  Versioned scenario artifacts, scenario execution, and scenario verification.
- `api`
  Stable consumer-facing entry points and sample engine construction.
- `bindings`
  Leaf wrappers for FFI, WASM, or host-specific integration.

### Dependency Direction

- `core`
  Depends on nothing else.
- `config`, `input`, `state`, `rng`
  May depend on `core` only.
- `validation`
  May depend on `core` and `state`.
- `replay`
  May depend on `core`, `input`, `state`, `validation`.
- `persistence`
  May depend on `core`, `config`, `input`, `state`, `replay`, `engine`, and
  `serialization`.
- `serialization`
  May depend on `core`, `config`, `state`, `input`, and `validation`.
- `phases`
  May depend on `core`, `input`, `state`, `rng`, `replay`.
- `scheduler`
  May depend on `core`, `phases`.
- `engine`
  May depend on `core`, `input`, `state`, `rng`, `replay`, `validation`,
  `serialization`, `scheduler`, `phases`.
- `scenario`
  May depend on `config`, `core`, `input`, `parity`, `persistence`, and
  `serialization`.
- `api`
  May depend on `engine` and public types from inner modules.
- `bindings`
  May depend on `api` only.

Rules:

- inner layers never depend on `api` or `bindings`
- `bindings` never reach into concrete engine internals directly
- `engine` owns orchestration, not policy hidden in adapters

## Core Abstractions

### Rust Pseudocode

```rust
trait Engine<I: Command> {
    type State: SimulationState;

    fn seed(&self) -> u64;
    fn state(&self) -> &Self::State;
    fn tick(&mut self, input: InputFrame<I>) -> Result<TickResult<Self::State>, EngineError>;
}

trait SimulationState: Clone {
    type Snapshot: Clone + Eq;

    fn tick(&self) -> u64;
    fn set_tick(&mut self, tick: u64);
    fn checksum(&self) -> u64;
    fn snapshot(&self) -> Self::Snapshot;
    fn restore_snapshot(&mut self, snapshot: Self::Snapshot);
}

trait Command: Clone + Eq + core::fmt::Debug {}

trait Rng: Clone {
    fn from_seed(seed: u64) -> Self;
    fn seed(&self) -> u64;
    fn next_u64(&mut self) -> u64;
    fn fork(&self, stream: &'static str) -> Self;
}

trait Phase<S, I, R, L>
where
    S: SimulationState,
    I: Command,
    R: Rng,
    L: ReplayLog<I, S::Snapshot>,
{
    fn name(&self) -> &'static str;
    fn run(&mut self, ctx: &mut TickContext<'_, S, I, R, L>) -> Result<(), EngineError>;
}

trait Scheduler<S, I, R, L>
where
    S: SimulationState,
    I: Command,
    R: Rng,
    L: ReplayLog<I, S::Snapshot>,
{
    fn visit_phases(
        &mut self,
        visitor: &mut dyn FnMut(
            PhaseDescriptor,
            &mut dyn Phase<S, I, R, L>,
        ) -> Result<(), EngineError>,
    ) -> Result<(), EngineError>;

    fn phase_plan(&self) -> Vec<PhaseDescriptor>;
}

trait ReplayLog<I: Command, Snapshot: Clone + Eq> {
    fn begin_tick(&mut self, frame: &InputFrame<I>, tick_seed: u64) -> Result<(), EngineError>;
    fn record_phase(&mut self, marker: PhaseMarker) -> Result<(), EngineError>;
    fn complete_tick(
        &mut self,
        checksum: u64,
        snapshot: Option<SnapshotRecord<Snapshot>>,
    ) -> Result<(), EngineError>;
    fn records(&self) -> &[ReplayTickRecord<I, Snapshot>];
}

trait Serializer<T> {
    type Error;

    fn schema_version(&self) -> u32;
    fn encode(&self, value: &T) -> Result<Vec<u8>, Self::Error>;
    fn decode(&self, bytes: &[u8]) -> Result<T, Self::Error>;
}
```

## Main Loop

### Per-Tick Data Flow

1. Caller passes an explicit `InputFrame`.
2. Engine validates strict ordered tick progression and fails fast on gaps or rewinds.
3. Engine treats typed config as part of deterministic identity and uses it to
   construct initial state, snapshot cadence policy, and validation policy.
4. Engine derives a deterministic tick seed from the base seed and tick number.
5. Engine begins an append-only replay record for that tick.
6. Engine runs explicit invariant validation at `BeforeTickBegin`.
7. Scheduler visits phases in stable grouped order:
   `PreInput -> Input -> Simulation -> PostSimulation -> Finalize`.
8. Engine runs validation again at group boundaries according to policy:
   `AfterInputApplied`, `AfterSimulationGroup`, and `AfterFinalize`.
9. Each phase mutates state through `TickContext`.
10. Each phase derives its own RNG stream from the tick seed plus a stable stream
    label.
11. Engine advances authoritative tick metadata after all phases succeed.
12. Engine computes the authoritative checksum.
13. Engine applies explicit snapshot cadence policy:
    `Never`, `Every { interval }`, or `Manual`.
14. Engine finalizes the replay tick record with checksum, validation summaries,
    phase markers, and optional snapshot payload plus snapshot metadata
    `(payload_schema_version, source_tick, capture_checksum)`.

### Main Loop Pseudocode

```rust
fn tick(&mut self, frame: InputFrame<I>) -> Result<TickResult<S>, EngineError> {
    let tick = self.validate_ordered_input(&frame)?;
    let tick_seed = self.derive_tick_seed(tick);
    self.replay.begin_tick(&frame, tick_seed)?;

    self.scheduler.visit_phases(&mut |descriptor, phase| {
        self.replay.record_phase(descriptor.into())?;
        let mut ctx = TickContext::new(
            self.seed,
            tick_seed,
            tick,
            &frame,
            &mut self.state,
            &mut self.replay,
        );
        phase.run(&mut ctx)
    })?;

    self.state.set_tick(tick);

    let checksum = self.state.checksum();
    let snapshot = self.snapshot_policy.should_capture(tick).map(|reason| {
        let payload = self.state.snapshot();
        SnapshotRecord {
            metadata: SnapshotMetadata {
                payload_schema_version: S::snapshot_schema_version(),
                source_tick: tick,
                capture_checksum: checksum,
            },
            reason,
            payload,
        }
    });

    self.replay.complete_tick(checksum, snapshot.clone())?;

    Ok(TickResult {
        tick,
        checksum,
        snapshot: snapshot.map(|record| record.payload),
    })
}
```

## RNG Design

### Hard Rules

- all randomness comes from a seeded deterministic RNG
- the base seed is explicit engine configuration
- no global RNG
- no runtime seeding from system time or OS entropy

### Seed Injection

- engine constructor receives `seed: u64`
- each tick derives `tick_seed = mix(seed, tick)`
- each phase or subsystem derives `stream_seed = mix(tick_seed, stable_stream_id)`

### Why Per-Phase Streams Matter

If all systems share a mutable RNG cursor, adding a random draw in one system can
shift every downstream draw. For a deterministic engine, that kind of coupling
is too fragile. A better baseline is:

- deterministic tick seed
- deterministic named stream per phase/system/domain

Then branching inside one phase affects only that phase’s local stream.

### Branching Strategy

- derive RNG streams from stable names such as `movement`, `combat`, `loot`
- keep stream naming part of deterministic contract
- do not derive stream names from unordered runtime data

## Replay Design

### What To Store

- engine seed
- ordered input frame per tick
- deterministic tick seed per tick
- phase execution markers
- per-tick checksum
- per-tick validation summaries and state digest progression at explicit
  checkpoints
- optional snapshots at configured intervals
- replay artifact metadata:
  base seed, config identity, total ticks, snapshot policy, command payload
  schema version, snapshot payload schema version, and replay artifact schema
  version

The current Rust baseline records replay as append-only tick records, not a flat
event stream. Each record contains:

- tick index
- input frame
- derived tick seed
- ordered phase markers with group + ordinal
- checksum
- validation summaries
- optional snapshot record with capture reason, schema version, source tick,
  capture checksum, and payload
- optional replay inspection view with deterministic per-tick summaries:
  phase markers, validation checkpoints, checksum, state digest progression,
  snapshot presence, and snapshot metadata pointers

### Modes

- record mode
  Run from seed with explicit input frames, record append-only replay records,
  and emit a deterministic replay artifact.
- replay verify mode
  Decode a replay artifact, rerun from the recorded seed and policy, and fail on
  the first divergence.
- replay from snapshot mode
  Restore a compatible snapshot artifact, validate replay continuation from the
  next tick, and continue execution through the remaining replay records.

### Debug Capability

- step-by-step replay through logged tick and phase boundaries
- divergence detection on first mismatch across input, tick seed, phase marker
  ordering, validation summaries, checksum, tick count, snapshot metadata, and
  snapshot payload digest
- snapshot-assisted bisecting when desync begins after a known checkpoint
- replay/snapshot artifacts suitable for golden fixtures and future Rust/C++
  parity checks

### Replay Inspection Surface

The current Rust baseline also exposes a deterministic inspection view over a
recorded trace:

- `ReplayInspectionView`
  Stable top-level trace summary with record count and final tick.
- `ReplayTickSummary`
  Per-tick input tick, derived tick seed, validation summaries, phase markers,
  checksum, state digest progression, and snapshot presence.

This surface is intentionally library-only. It exists to make test failures and
desync investigation easier without weakening replay strictness.

## Serialization

### Model

Snapshots and replay artifacts must be serializable from semantic fields, not
raw memory.

Recommended baseline:

- explicit serializer trait
- explicit config payload schema version
- explicit payload schema version
- explicit replay artifact schema version
- explicit snapshot artifact schema version
- deterministic field ordering
- integer/fixed-width numeric encoding
- canonical delimiter and escaping rules
- no ambiguous optional fields
- export -> import -> export must be byte-for-byte identical

The current baseline uses deterministic text artifacts because they are easy to
inspect, diff, and version while keeping memory-layout dependence out of the
design. Binary encoding can come later, but it should still be field-oriented
and versioned.

The current Rust implementation hardens that text encoding with:

- fixed `artifact=` / `canonical_encoding=` headers
- fixed field order per artifact and payload type
- hex encoding for free-form strings and nested artifact bytes
- fail-fast decode for duplicate keys, missing required keys, invalid field
  order, unsupported schema versions, and trailing malformed content
- canonical digest generation from exported bytes for replay, snapshot, and
  fixture parity checks

### Versioning Constraint

Replay validity across versions is only possible when:

- input payload schema remains compatible or versioned
- snapshot payload schema is versioned
- replay artifact schema is versioned
- snapshot artifact schema is versioned
- checksum semantics are stable or explicitly versioned

If compatibility is not guaranteed, fail fast instead of silently attempting
replay with mismatched state layouts.

Artifact schema versions and payload schema versions must stay separate.
Artifacts describe bundle/container layout. Payload schemas describe config,
command, and snapshot contracts. The decoder must never guess between them.

## Golden Fixtures + Parity Prep

The Rust baseline now treats golden fixtures as a first-class library surface.

- `GoldenFixture`
  Versioned bundle containing config artifact, optional scenario artifact,
  replay artifact, optional snapshot artifact, and a summary.
- `GoldenFixtureSummary`
  Stable summary with config identity, seed, final tick, final checksum, schema
  versions, replay digest, optional snapshot digest, and optional scenario
  digest.
- `GoldenFixtureResult`
  Verification report containing summary comparison plus separated config,
  replay, and snapshot mismatch details.
- `SimulationScenario`
  Versioned scenario contract over config artifact, seed, ordered frames, and
  optional expected parity summary.
- `ParityArtifactSummary`
  Minimal digest-oriented summary for future Rust/C++ comparison, now including
  config schema/digest and optional scenario digest.
- `ParityComparison`
  Ordered mismatch list across config schema/digest, base seed, final tick,
  final checksum, replay digest, optional snapshot digest, and optional
  scenario digest.

This is deliberately library-level only. There is no large file-I/O layer here.
The goal is to make parity fixtures deterministic, inspectable, and easy to
assert in tests before any external Rust/C++ integration is attempted.

## Determinism Rules

- no system time
- no wall-clock delta time
- no hidden state outside engine-owned objects
- no global mutable singletons
- no ambient randomness
- no unordered iteration where order affects behavior
- no floating-point nondeterminism in authoritative state unless the strategy is
  explicitly controlled
- all input must be explicit and ordered
- all seeds must be explicit
- all external side effects must sit outside authoritative state evolution
- snapshots and checksums must derive from semantic state, not object layout
- scheduler order must be explicit and stable

## Risks + Mitigation

- floating-point drift
  Prefer integers or fixed-point for authoritative state. If floats are
  unavoidable, isolate them from authoritative checksums and snapshots.
- parallelism vs determinism
  Keep the default engine single-threaded. Introduce parallelism only behind
  deterministic partitioning and explicit merge rules.
- platform differences between Rust and C++
  Keep shared types fixed-width, avoid UB-dependent behavior, and validate
  checksums across implementations with golden replay fixtures.
- serialization incompatibility
  Version schemas explicitly and reject unsupported versions early.
- replay desync
  Log checksums per tick, optionally emit snapshots, and provide binary-search
  style replay debugging around divergence points.

## Roadmap

### Phase 1: Core Loop + State + Input

- deliverable
  `Engine`, `SimulationState`, `InputFrame`, minimal fixed scheduler
- test strategy
  one-tick and multi-tick deterministic state tests

### Phase 2: RNG + Determinism Enforcement

- deliverable
  seeded RNG, tick-seed derivation, named stream forking
- test strategy
  same-seed/same-input equality, changed-seed inequality, stable stream tests

### Phase 3: Replay System

- deliverable
  replay log trait, in-memory log, divergence metadata
- test strategy
  compare replay traces across repeated runs

### Phase 4: Serialization + Snapshot

- deliverable
  deterministic serializer, snapshot cadence, restore path
- test strategy
  snapshot roundtrip and restore-and-continue determinism

### Phase 5: Scheduler + Phases

- deliverable
  phase ordering, scheduler boundary, phase-level replay markers
- test strategy
  ordering tests and checksum stability under unchanged phase order

### Phase 6: API + Bindings

- deliverable
  stable host-facing API and thin binding wrappers for `xenor-native` and
  future web/WASM surfaces
- test strategy
  API-level replay fixture tests and binding smoke tests

## Current Rust Baseline

The repository now includes an isolated Rust core under `rust/` with:

- explicit module boundaries matching this document
- typed simulation config artifacts with canonical identity digests
- a deterministic tick pipeline with strict ordered-input validation
- grouped fixed scheduler inspection through `PhaseDescriptor` and `PhaseGroup`
- append-only replay tick records with phase markers, validation summaries, and
  richer divergence checking
- explicit snapshot cadence policy through `SnapshotPolicy`
- versioned replay artifact persistence with deterministic encode/decode
- versioned snapshot artifacts with metadata validation on restore
- canonical scenario artifacts plus scenario execution/verification helpers
- canonical golden fixture generation, import/export, and config-aware
  verification
- parity summary and comparison helpers for future Rust/C++ fixture work,
  including config identity
- replay inspection views for deterministic mismatch debugging
- explicit invariant validation policy at tick boundaries or every phase group
- fast-forward replay verification from compatible snapshot checkpoints
- one example state type and command pipeline that makes phase ordering observable
- one deterministic serializer example
- regression tests for replay equality, canonical re-export, schema version
  gating, config/scenario roundtrips, golden fixture verification,
  snapshot-backed resume, replay inspection, invariant enforcement, scenario
  execution, parity summaries, and richer replay divergence reporting

This Rust core remains intentionally narrow. It is a deterministic architecture
and validation surface for future native and web bindings, not a replacement
for the existing C++ runtime today.
