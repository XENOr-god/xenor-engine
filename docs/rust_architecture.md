# Rust-Oriented Deterministic Engine Architecture

## Goals

- deterministic-first execution from `seed + ordered input frames`
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

`seed + ordered input frames + phase ordering + serializer version -> identical snapshots, checksums, and replay results`

The engine is best treated as a deterministic execution kernel composed of:

- `core`
  Shared deterministic types, error model, seed/tick helpers, and checksum
  utilities. This layer does not depend on runtime orchestration.
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
- `replay`
  Deterministic recording of inputs, phase execution, snapshots, and checksums.
- `serialization`
  Deterministic snapshot encoding and decoding with explicit schema versioning.
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
    state/
    input/
    rng/
    replay/
    serialization/
    phases/
    scheduler/
    engine/
    api/
    bindings/
```

### Module Responsibilities

- `core`
  Seed/tick/checksum helpers, deterministic errors, shared scalar types.
- `state`
  `SimulationState` contract and canonical snapshot boundary inside the Rust
  architecture.
- `input`
  `Command` trait and `InputFrame`.
- `rng`
  `Rng` trait and seeded deterministic implementations.
- `replay`
  `ReplayLog` trait plus in-memory log implementation for tests and debugging.
- `serialization`
  `Serializer` trait and deterministic schema-oriented encoding examples.
- `phases`
  `Phase` trait and `TickContext`.
- `scheduler`
  `Scheduler` trait and fixed-order scheduler implementation.
- `engine`
  `Engine` trait and concrete deterministic engine loop.
- `api`
  Stable consumer-facing entry points and sample engine construction.
- `bindings`
  Leaf wrappers for FFI, WASM, or host-specific integration.

### Dependency Direction

- `core`
  Depends on nothing else.
- `input`, `state`, `rng`
  May depend on `core` only.
- `replay`
  May depend on `core`, `input`, `state`.
- `serialization`
  May depend on `core`, `state`, `replay`.
- `phases`
  May depend on `core`, `input`, `state`, `rng`, `replay`.
- `scheduler`
  May depend on `core`, `phases`.
- `engine`
  May depend on `core`, `input`, `state`, `rng`, `replay`, `serialization`,
  `scheduler`, `phases`.
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
3. Engine derives a deterministic tick seed from the base seed and tick number.
4. Engine begins an append-only replay record for that tick.
5. Scheduler visits phases in stable grouped order:
   `PreInput -> Input -> Simulation -> PostSimulation -> Finalize`.
6. Each phase mutates state through `TickContext`.
7. Each phase derives its own RNG stream from the tick seed plus a stable stream
   label.
8. Engine advances authoritative tick metadata after all phases succeed.
9. Engine computes the authoritative checksum.
10. Engine applies explicit snapshot cadence policy:
    `Never`, `Every { interval }`, or `Manual`.
11. Engine finalizes the replay tick record with checksum, phase markers, and
    optional snapshot payload.

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
    let snapshot = self.snapshot_policy.should_capture(tick).map(|reason| SnapshotRecord {
        tick,
        reason,
        payload: self.state.snapshot(),
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
- optional snapshots at configured intervals

The current Rust baseline records replay as append-only tick records, not a flat
event stream. Each record contains:

- tick index
- input frame
- derived tick seed
- ordered phase markers with group + ordinal
- checksum
- optional snapshot record with capture reason and payload

### Modes

- full replay
  Start from the seed and replay every input frame from tick 1.
- fast-forward replay
  Load the latest compatible snapshot, then replay the remaining input frames.

### Debug Capability

- step-by-step replay through logged tick and phase boundaries
- divergence detection on first mismatch across input, tick seed, phase marker
  ordering, checksum, and snapshot presence or payload
- snapshot-assisted bisecting when desync begins after a known checkpoint

## Serialization

### Model

Snapshots must be serializable from semantic fields, not raw memory.

Recommended baseline:

- explicit serializer trait
- explicit schema version
- deterministic field ordering
- integer/fixed-width numeric encoding

The skeleton includes a deterministic text serializer example because it is easy
to inspect and version while keeping memory-layout dependence out of the design.
Binary encoding can come later, but it should still be field-oriented and
versioned.

### Versioning Constraint

Replay validity across versions is only possible when:

- input schema remains compatible or versioned
- snapshot schema is versioned
- checksum semantics are stable or explicitly versioned

If compatibility is not guaranteed, fail fast instead of silently attempting
replay with mismatched state layouts.

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
- a deterministic tick pipeline with strict ordered-input validation
- grouped fixed scheduler inspection through `PhaseDescriptor` and `PhaseGroup`
- append-only replay tick records with phase markers and divergence checking
- explicit snapshot cadence policy through `SnapshotPolicy`
- one example state type and command pipeline that makes phase ordering observable
- one deterministic serializer example
- regression tests for replay equality, tick-order failure, snapshot cadence,
  restore-and-continue, and replay divergence reporting

This Rust core remains intentionally narrow. It is a deterministic architecture
and validation surface for future native and web bindings, not a replacement
for the existing C++ runtime today.
