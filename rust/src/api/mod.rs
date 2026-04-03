use crate::core::{EngineError, Seed, Tick, mix64};
use crate::engine::{DeterministicEngine, Engine, SnapshotPolicy};
use crate::fixture::{GoldenFixture, GoldenFixtureResult, GoldenFixtureSerializer};
use crate::input::{Command, InputFrame};
use crate::parity::ParityArtifactSummary;
use crate::persistence::{
    ArtifactSummary, RecordedReplay, ReplayArtifact, ReplayArtifactSerializer,
    ReplayExecutionResult, SnapshotArtifact, SnapshotArtifactSerializer,
    execute_replay_from_snapshot, execute_replay_verify, record_replay,
};
use crate::phases::{Phase, TickContext};
use crate::replay::{InMemoryReplayLog, ReplayInspectionView, inspect_replay_trace};
use crate::rng::{Rng, SplitMix64};
use crate::scheduler::{FixedScheduler, PhaseGroup};
use crate::serialization::{
    CounterCommandTextSerializer, CounterSnapshotTextSerializer, Serializer,
};
use crate::state::{CounterSnapshot, CounterState, SimulationState};

pub const COUNTER_ENGINE_FAMILY: &str = "xenor-engine-rust/counter";

pub trait EngineApi<C: Command>: Engine<C> {
    fn snapshot(&self) -> <Self::State as SimulationState>::Snapshot {
        self.state().snapshot()
    }
}

impl<T, C> EngineApi<C> for T
where
    T: Engine<C>,
    C: Command,
{
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterCommand {
    pub delta: i64,
    pub consume_entropy: bool,
}

pub type CounterReplayLog = InMemoryReplayLog<CounterCommand, CounterSnapshot>;
pub type CounterScheduler =
    FixedScheduler<CounterState, CounterCommand, SplitMix64, CounterReplayLog>;
pub type CounterEngine = DeterministicEngine<
    CounterState,
    CounterCommand,
    SplitMix64,
    CounterReplayLog,
    CounterScheduler,
>;
pub type CounterReplayArtifact = ReplayArtifact<CounterCommand, CounterSnapshot>;
pub type CounterSnapshotArtifact = SnapshotArtifact<CounterSnapshot>;
pub type CounterRecordedReplay = RecordedReplay<CounterCommand, CounterSnapshot>;
pub type CounterReplayResult = ReplayExecutionResult<CounterSnapshot>;
pub type CounterReplayArtifactCodec = ReplayArtifactSerializer<
    CounterCommand,
    CounterSnapshot,
    CounterCommandTextSerializer,
    CounterSnapshotTextSerializer,
>;
pub type CounterSnapshotArtifactCodec =
    SnapshotArtifactSerializer<CounterSnapshot, CounterSnapshotTextSerializer>;
pub type CounterGoldenFixture = GoldenFixture<CounterCommand, CounterSnapshot>;
pub type CounterGoldenFixtureResult = GoldenFixtureResult;
pub type CounterGoldenFixtureCodec = GoldenFixtureSerializer<
    CounterCommand,
    CounterSnapshot,
    CounterCommandTextSerializer,
    CounterSnapshotTextSerializer,
>;
pub type CounterParitySummary = ParityArtifactSummary;

#[derive(Default)]
pub struct ResetFinalizeMarkerPhase;

impl Phase<CounterState, CounterCommand, SplitMix64, CounterReplayLog>
    for ResetFinalizeMarkerPhase
{
    fn name(&self) -> &'static str {
        "reset_finalize_marker"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, CounterState, CounterCommand, SplitMix64, CounterReplayLog>,
    ) -> Result<(), EngineError> {
        ctx.state.reset_finalize_marker();
        Ok(())
    }
}

#[derive(Default)]
pub struct ApplyCounterInputPhase;

impl Phase<CounterState, CounterCommand, SplitMix64, CounterReplayLog> for ApplyCounterInputPhase {
    fn name(&self) -> &'static str {
        "apply_counter_input"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, CounterState, CounterCommand, SplitMix64, CounterReplayLog>,
    ) -> Result<(), EngineError> {
        let entropy = if ctx.frame.command.consume_entropy {
            let mut rng = ctx.rng_for(self.name());
            rng.next_u64() & 0xff
        } else {
            0
        };

        ctx.state.stage_input(ctx.frame.command.delta, entropy);
        Ok(())
    }
}

#[derive(Default)]
pub struct SimulateCounterPhase;

impl Phase<CounterState, CounterCommand, SplitMix64, CounterReplayLog> for SimulateCounterPhase {
    fn name(&self) -> &'static str {
        "simulate_counter"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, CounterState, CounterCommand, SplitMix64, CounterReplayLog>,
    ) -> Result<(), EngineError> {
        ctx.state.simulate();
        Ok(())
    }
}

#[derive(Default)]
pub struct SettleCounterPhase;

impl Phase<CounterState, CounterCommand, SplitMix64, CounterReplayLog> for SettleCounterPhase {
    fn name(&self) -> &'static str {
        "settle_counter"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, CounterState, CounterCommand, SplitMix64, CounterReplayLog>,
    ) -> Result<(), EngineError> {
        ctx.state.settle();
        Ok(())
    }
}

#[derive(Default)]
pub struct FinalizeCounterPhase;

impl Phase<CounterState, CounterCommand, SplitMix64, CounterReplayLog> for FinalizeCounterPhase {
    fn name(&self) -> &'static str {
        "finalize_counter"
    }

    fn run(
        &mut self,
        ctx: &mut TickContext<'_, CounterState, CounterCommand, SplitMix64, CounterReplayLog>,
    ) -> Result<(), EngineError> {
        let marker = mix64(ctx.tick_seed ^ ctx.state.checksum() ^ ctx.tick);
        ctx.state.finalize(marker);
        Ok(())
    }
}

pub(crate) fn default_counter_scheduler() -> CounterScheduler {
    let mut scheduler = CounterScheduler::new();
    scheduler.add_phase(PhaseGroup::PreInput, ResetFinalizeMarkerPhase);
    scheduler.add_phase(PhaseGroup::Input, ApplyCounterInputPhase);
    scheduler.add_phase(PhaseGroup::Simulation, SimulateCounterPhase);
    scheduler.add_phase(PhaseGroup::PostSimulation, SettleCounterPhase);
    scheduler.add_phase(PhaseGroup::Finalize, FinalizeCounterPhase);
    scheduler
}

#[cfg(test)]
pub(crate) fn reordered_counter_scheduler() -> CounterScheduler {
    let mut scheduler = CounterScheduler::new();
    scheduler.add_phase(PhaseGroup::PreInput, ResetFinalizeMarkerPhase);
    scheduler.add_phase(PhaseGroup::Input, SimulateCounterPhase);
    scheduler.add_phase(PhaseGroup::Simulation, ApplyCounterInputPhase);
    scheduler.add_phase(PhaseGroup::PostSimulation, SettleCounterPhase);
    scheduler.add_phase(PhaseGroup::Finalize, FinalizeCounterPhase);
    scheduler
}

pub(crate) fn build_counter_engine(
    seed: Seed,
    scheduler: CounterScheduler,
    snapshot_policy: SnapshotPolicy,
) -> CounterEngine {
    DeterministicEngine::new(
        seed,
        CounterState::default(),
        scheduler,
        CounterReplayLog::default(),
    )
    .with_snapshot_policy(snapshot_policy)
}

pub fn counter_engine_with_policy(seed: Seed, snapshot_policy: SnapshotPolicy) -> CounterEngine {
    build_counter_engine(seed, default_counter_scheduler(), snapshot_policy)
}

pub fn minimal_counter_engine(seed: Seed) -> CounterEngine {
    counter_engine_with_policy(seed, SnapshotPolicy::Every { interval: 1 })
}

pub fn counter_replay_artifact_codec() -> CounterReplayArtifactCodec {
    ReplayArtifactSerializer::new(
        COUNTER_ENGINE_FAMILY,
        CounterCommandTextSerializer,
        CounterSnapshotTextSerializer,
    )
}

pub fn counter_snapshot_artifact_codec() -> CounterSnapshotArtifactCodec {
    SnapshotArtifactSerializer::new(COUNTER_ENGINE_FAMILY, CounterSnapshotTextSerializer)
}

pub fn counter_golden_fixture_codec() -> CounterGoldenFixtureCodec {
    GoldenFixtureSerializer::new(
        counter_replay_artifact_codec(),
        counter_snapshot_artifact_codec(),
    )
}

pub fn record_counter_replay(
    seed: Seed,
    snapshot_policy: SnapshotPolicy,
    frames: &[InputFrame<CounterCommand>],
) -> Result<CounterRecordedReplay, EngineError> {
    let codec = counter_replay_artifact_codec();
    record_replay::<CounterCommand, CounterState, CounterEngine, _, _, _>(
        seed,
        snapshot_policy,
        frames,
        counter_engine_with_policy,
        &codec,
    )
}

pub fn export_counter_replay_artifact(
    artifact: &CounterReplayArtifact,
) -> Result<Vec<u8>, EngineError> {
    counter_replay_artifact_codec().encode(artifact)
}

pub fn import_counter_replay_artifact(bytes: &[u8]) -> Result<CounterReplayArtifact, EngineError> {
    counter_replay_artifact_codec().decode(bytes)
}

pub fn counter_replay_summary(
    artifact: &CounterReplayArtifact,
) -> Result<ArtifactSummary, EngineError> {
    counter_replay_artifact_codec().summary(artifact)
}

pub fn counter_parity_summary(
    artifact: &CounterReplayArtifact,
) -> Result<CounterParitySummary, EngineError> {
    counter_replay_summary(artifact).map(|summary| CounterParitySummary::from(&summary))
}

pub fn inspect_counter_replay(artifact: &CounterReplayArtifact) -> ReplayInspectionView {
    inspect_replay_trace(artifact.records.as_slice())
}

pub fn verify_counter_replay(
    artifact: &CounterReplayArtifact,
) -> Result<CounterReplayResult, EngineError> {
    let codec = counter_replay_artifact_codec();
    execute_replay_verify::<CounterCommand, CounterState, CounterEngine, _, _, _>(
        artifact,
        counter_engine_with_policy,
        &codec,
    )
}

pub fn counter_snapshot_artifact_from_engine(engine: &CounterEngine) -> CounterSnapshotArtifact {
    counter_snapshot_artifact_codec().build_artifact(engine.seed(), &engine.manual_snapshot())
}

pub fn counter_snapshot_artifact_at_tick(
    artifact: &CounterReplayArtifact,
    tick: Tick,
) -> Result<CounterSnapshotArtifact, EngineError> {
    let snapshot = artifact
        .records
        .iter()
        .find(|record| record.tick == tick)
        .and_then(|record| record.snapshot.as_ref())
        .ok_or_else(|| EngineError::ReplayContinuationMismatch {
            detail: format!("no snapshot recorded at tick {tick}"),
        })?;

    Ok(counter_snapshot_artifact_codec().build_artifact(artifact.metadata.base_seed, snapshot))
}

pub fn export_counter_snapshot_artifact(
    artifact: &CounterSnapshotArtifact,
) -> Result<Vec<u8>, EngineError> {
    counter_snapshot_artifact_codec().encode(artifact)
}

pub fn import_counter_snapshot_artifact(
    bytes: &[u8],
) -> Result<CounterSnapshotArtifact, EngineError> {
    counter_snapshot_artifact_codec().decode(bytes)
}

pub fn generate_counter_golden_fixture(
    seed: Seed,
    snapshot_policy: SnapshotPolicy,
    frames: &[InputFrame<CounterCommand>],
) -> Result<CounterGoldenFixture, EngineError> {
    counter_golden_fixture_codec().generate_fixture::<CounterState, CounterEngine, _>(
        seed,
        snapshot_policy,
        frames,
        counter_engine_with_policy,
    )
}

pub fn export_counter_golden_fixture(
    fixture: &CounterGoldenFixture,
) -> Result<Vec<u8>, EngineError> {
    counter_golden_fixture_codec().encode(fixture)
}

pub fn import_counter_golden_fixture(bytes: &[u8]) -> Result<CounterGoldenFixture, EngineError> {
    counter_golden_fixture_codec().decode(bytes)
}

pub fn verify_counter_golden_fixture(
    fixture: &CounterGoldenFixture,
) -> Result<CounterGoldenFixtureResult, EngineError> {
    counter_golden_fixture_codec()
        .verify_fixture::<CounterState, CounterEngine, _>(fixture, counter_engine_with_policy)
}

pub fn resume_counter_replay_from_snapshot(
    snapshot: &CounterSnapshotArtifact,
    artifact: &CounterReplayArtifact,
) -> Result<CounterReplayResult, EngineError> {
    let codec = counter_replay_artifact_codec();
    execute_replay_from_snapshot::<CounterCommand, CounterState, CounterEngine, _, _, _>(
        snapshot,
        artifact,
        counter_engine_with_policy,
        &codec,
    )
}

pub fn one_tick_counter_snapshot(seed: Seed, delta: i64) -> CounterSnapshot {
    let mut engine = minimal_counter_engine(seed);
    engine
        .tick(InputFrame::new(
            1,
            CounterCommand {
                delta,
                consume_entropy: true,
            },
        ))
        .expect("minimal counter engine should tick deterministically");
    engine.snapshot()
}
