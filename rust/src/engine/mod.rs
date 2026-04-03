use std::marker::PhantomData;

use crate::core::{EngineError, Seed, Tick, tick_seed};
use crate::input::{Command, InputFrame};
use crate::phases::{Phase, TickContext};
use crate::replay::{
    ReplayLog, ReplayTickRecord, SnapshotCaptureReason, SnapshotMetadata, SnapshotRecord,
};
use crate::rng::Rng;
use crate::scheduler::{FixedScheduler, PhaseDescriptor, Scheduler};
use crate::serialization::Serializer;
use crate::state::SimulationState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotPolicy {
    Never,
    Every { interval: Tick },
    Manual,
}

impl SnapshotPolicy {
    pub const fn should_capture(self, tick: Tick) -> Option<SnapshotCaptureReason> {
        match self {
            Self::Never | Self::Manual => None,
            Self::Every { interval } if interval != 0 && tick % interval == 0 => {
                Some(SnapshotCaptureReason::PolicyInterval { interval })
            }
            Self::Every { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickResult<S: SimulationState> {
    pub tick: Tick,
    pub checksum: u64,
    pub snapshot: Option<S::Snapshot>,
}

pub trait Engine<C: Command> {
    type State: SimulationState;

    fn seed(&self) -> Seed;
    fn state(&self) -> &Self::State;
    fn tick(&mut self, frame: InputFrame<C>) -> Result<TickResult<Self::State>, EngineError>;
}

pub trait ReplayableEngine<C: Command>: Engine<C> {
    fn snapshot_policy(&self) -> SnapshotPolicy;
    fn restore_snapshot(&mut self, snapshot: <Self::State as SimulationState>::Snapshot);
    fn replay_records(&self) -> &[ReplayTickRecord<C, <Self::State as SimulationState>::Snapshot>];
}

pub struct DeterministicEngine<S, C, R, L, Sch = FixedScheduler<S, C, R, L>>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
    Sch: Scheduler<S, C, R, L>,
{
    seed: Seed,
    state: S,
    replay_log: L,
    scheduler: Sch,
    snapshot_policy: SnapshotPolicy,
    _marker: PhantomData<(C, R)>,
}

impl<S, C, R, L, Sch> DeterministicEngine<S, C, R, L, Sch>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
    Sch: Scheduler<S, C, R, L>,
{
    pub fn new(seed: Seed, state: S, scheduler: Sch, replay_log: L) -> Self {
        Self {
            seed,
            state,
            replay_log,
            scheduler,
            snapshot_policy: SnapshotPolicy::Never,
            _marker: PhantomData,
        }
    }

    pub fn with_snapshot_policy(mut self, snapshot_policy: SnapshotPolicy) -> Self {
        self.snapshot_policy = snapshot_policy;
        self
    }

    pub fn replay_log(&self) -> &L {
        &self.replay_log
    }

    pub fn scheduler(&self) -> &Sch {
        &self.scheduler
    }

    pub const fn snapshot_policy(&self) -> SnapshotPolicy {
        self.snapshot_policy
    }

    pub fn manual_snapshot(&self) -> SnapshotRecord<S::Snapshot> {
        let payload = self.state.snapshot();
        SnapshotRecord {
            metadata: SnapshotMetadata {
                payload_schema_version: S::snapshot_schema_version(),
                source_tick: S::snapshot_tick(&payload),
                capture_checksum: S::snapshot_checksum(&payload),
            },
            reason: SnapshotCaptureReason::Manual,
            payload,
        }
    }

    pub fn restore_snapshot(&mut self, snapshot: S::Snapshot) {
        self.state.restore_snapshot(snapshot);
    }

    pub fn serialize_snapshot_with<T>(&self, serializer: &T) -> Result<Vec<u8>, EngineError>
    where
        T: Serializer<S::Snapshot>,
        T::Error: std::fmt::Display,
    {
        let snapshot = self.manual_snapshot();
        serializer
            .encode(&snapshot.payload)
            .map_err(|error| EngineError::SnapshotSerialization {
                tick: snapshot.metadata.source_tick,
                reason: error.to_string(),
            })
    }

    fn validate_ordered_input(&self, frame: &InputFrame<C>) -> Result<Tick, EngineError> {
        let expected_tick = self.state.tick().saturating_add(1);
        if frame.tick != expected_tick {
            return Err(EngineError::UnexpectedInputTick {
                expected: expected_tick,
                got: frame.tick,
            });
        }

        Ok(frame.tick)
    }

    fn derive_tick_seed(&self, tick: Tick) -> Seed {
        tick_seed(self.seed, tick)
    }

    fn record_replay_input_begin(
        &mut self,
        frame: &InputFrame<C>,
        tick_seed: Seed,
    ) -> Result<(), EngineError> {
        self.replay_log.begin_tick(frame, tick_seed)
    }

    fn run_scheduler_phases(
        &mut self,
        frame: &InputFrame<C>,
        tick: Tick,
        tick_seed: Seed,
    ) -> Result<(), EngineError> {
        let seed = self.seed;
        let state = &mut self.state;
        let replay = &mut self.replay_log;

        let mut visitor = |descriptor: PhaseDescriptor,
                           phase: &mut dyn Phase<S, C, R, L>|
         -> Result<(), EngineError> {
            replay.record_phase(descriptor.into())?;

            let mut context =
                TickContext::new(seed, tick_seed, tick, frame, &mut *state, &mut *replay);
            phase
                .run(&mut context)
                .map_err(|error| EngineError::PhaseFailed {
                    tick,
                    group: descriptor.group.as_str(),
                    phase: descriptor.name,
                    reason: error.to_string(),
                })
        };

        self.scheduler.visit_phases(&mut visitor)
    }

    fn advance_authoritative_tick(&mut self, tick: Tick) {
        self.state.set_tick(tick);
    }

    fn compute_checksum(&self) -> u64 {
        self.state.checksum()
    }

    fn apply_snapshot_policy(
        &self,
        tick: Tick,
        checksum: u64,
    ) -> Option<SnapshotRecord<S::Snapshot>> {
        self.snapshot_policy.should_capture(tick).map(|reason| {
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
        })
    }

    fn finalize_replay_record(
        &mut self,
        checksum: u64,
        snapshot: Option<SnapshotRecord<S::Snapshot>>,
    ) -> Result<(), EngineError> {
        self.replay_log.complete_tick(checksum, snapshot)
    }
}

impl<S, C, R, L, Sch> Engine<C> for DeterministicEngine<S, C, R, L, Sch>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
    Sch: Scheduler<S, C, R, L>,
{
    type State = S;

    fn seed(&self) -> Seed {
        self.seed
    }

    fn state(&self) -> &Self::State {
        &self.state
    }

    fn tick(&mut self, frame: InputFrame<C>) -> Result<TickResult<Self::State>, EngineError> {
        let tick = self.validate_ordered_input(&frame)?;
        let tick_seed = self.derive_tick_seed(tick);
        self.record_replay_input_begin(&frame, tick_seed)?;
        self.run_scheduler_phases(&frame, tick, tick_seed)?;
        self.advance_authoritative_tick(tick);
        let checksum = self.compute_checksum();
        let snapshot_record = self.apply_snapshot_policy(tick, checksum);
        let snapshot_payload = snapshot_record
            .as_ref()
            .map(|snapshot| snapshot.payload.clone());
        self.finalize_replay_record(checksum, snapshot_record)?;

        Ok(TickResult {
            tick,
            checksum,
            snapshot: snapshot_payload,
        })
    }
}

impl<S, C, R, L, Sch> ReplayableEngine<C> for DeterministicEngine<S, C, R, L, Sch>
where
    S: SimulationState,
    C: Command,
    R: Rng,
    L: ReplayLog<C, S::Snapshot>,
    Sch: Scheduler<S, C, R, L>,
{
    fn snapshot_policy(&self) -> SnapshotPolicy {
        self.snapshot_policy
    }

    fn restore_snapshot(&mut self, snapshot: <Self::State as SimulationState>::Snapshot) {
        DeterministicEngine::restore_snapshot(self, snapshot);
    }

    fn replay_records(&self) -> &[ReplayTickRecord<C, <Self::State as SimulationState>::Snapshot>] {
        self.replay_log.records()
    }
}
