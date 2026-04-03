use crate::config::{ConfigArtifact, ConfigArtifactSerializer, CounterSimulationConfig};
use crate::core::{EngineError, Seed, Tick, mix64};
use crate::engine::{DeterministicEngine, Engine, SnapshotPolicy};
use crate::fixture::{
    GoldenFixture, GoldenFixtureResult, GoldenFixtureSerializer, GoldenFixtureSummary,
};
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
use crate::scenario::{
    ScenarioExecutionResult, ScenarioVerificationResult, SimulationScenario,
    SimulationScenarioSerializer,
};
use crate::scheduler::{FixedScheduler, PhaseGroup};
use crate::serialization::{
    CounterCommandTextSerializer, CounterConfigTextSerializer, CounterSnapshotTextSerializer,
    Serializer,
};
use crate::state::{CounterSnapshot, CounterState, SimulationState};
use crate::validation::{StateValidator, ValidationCheckpoint, ValidationContext};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterStateValidator {
    seed: Seed,
    config: CounterSimulationConfig,
}

impl CounterStateValidator {
    pub fn new(seed: Seed, config: CounterSimulationConfig) -> Self {
        Self { seed, config }
    }

    fn invariant_error(
        &self,
        context: ValidationContext,
        detail: impl Into<String>,
    ) -> EngineError {
        EngineError::InvariantViolation {
            tick: context.tick,
            checkpoint: context.checkpoint.as_str(),
            detail: detail.into(),
        }
    }

    fn expect_tick_alignment(
        &self,
        context: ValidationContext,
        state: &CounterState,
    ) -> Result<(), EngineError> {
        let expected_state_tick = context.tick.saturating_sub(1);
        if state.tick() != expected_state_tick {
            return Err(self.invariant_error(
                context,
                format!(
                    "authoritative tick mismatch: expected {}, got {}",
                    expected_state_tick,
                    state.tick()
                ),
            ));
        }

        Ok(())
    }

    fn expect_within_limit(
        &self,
        context: ValidationContext,
        label: &str,
        value: i64,
        limit: i64,
    ) -> Result<(), EngineError> {
        let magnitude = i128::from(value).abs();
        let limit = i128::from(limit);
        if magnitude > limit {
            return Err(self.invariant_error(
                context,
                format!("{label} exceeded deterministic limit: value={value}, limit={limit}"),
            ));
        }

        Ok(())
    }

    fn expect_limits(
        &self,
        context: ValidationContext,
        state: &CounterState,
    ) -> Result<(), EngineError> {
        self.expect_within_limit(context, "value", state.value(), self.config.max_abs_value)?;
        self.expect_within_limit(
            context,
            "velocity",
            state.velocity(),
            self.config.max_abs_velocity,
        )?;
        self.expect_within_limit(
            context,
            "pending_delta",
            state.pending_delta(),
            self.config.max_abs_pending_delta,
        )
    }

    fn expect_pending_cleared(
        &self,
        context: ValidationContext,
        state: &CounterState,
    ) -> Result<(), EngineError> {
        if state.pending_delta() != 0 {
            return Err(self.invariant_error(
                context,
                format!("pending_delta must be 0, got {}", state.pending_delta()),
            ));
        }

        if state.pending_entropy() != 0 {
            return Err(self.invariant_error(
                context,
                format!("pending_entropy must be 0, got {}", state.pending_entropy()),
            ));
        }

        Ok(())
    }

    fn expect_entity_store(
        &self,
        context: ValidationContext,
        state: &CounterState,
    ) -> Result<(), EngineError> {
        if state.entity_count() == 0 {
            return Err(
                self.invariant_error(context, "entity store must contain at least one entity")
            );
        }

        if state.entity_snapshot(state.primary_entity_id()).is_none() {
            return Err(self.invariant_error(
                context,
                format!(
                    "primary entity {:?} is missing from deterministic store",
                    state.primary_entity_id()
                ),
            ));
        }

        let entity_ids = state.entity_ids().collect::<Vec<_>>();
        if entity_ids.first().copied() != Some(state.primary_entity_id()) {
            return Err(self.invariant_error(
                context,
                format!(
                    "primary entity {:?} must remain first in insertion order, got {:?}",
                    state.primary_entity_id(),
                    entity_ids.first().copied()
                ),
            ));
        }

        let max_entity_id = entity_ids.iter().map(|id| id.0).max().unwrap_or(0);
        if state.next_entity_id().0 <= max_entity_id {
            return Err(self.invariant_error(
                context,
                format!(
                    "next entity id must be greater than all assigned ids: next={}, max={max_entity_id}",
                    state.next_entity_id().0
                ),
            ));
        }

        Ok(())
    }

    fn expect_marker_zero(
        &self,
        context: ValidationContext,
        state: &CounterState,
    ) -> Result<(), EngineError> {
        if state.finalize_marker() != 0 {
            return Err(self.invariant_error(
                context,
                format!(
                    "finalize_marker must be cleared, got {}",
                    state.finalize_marker()
                ),
            ));
        }

        Ok(())
    }

    fn expect_previous_marker(
        &self,
        context: ValidationContext,
        state: &CounterState,
    ) -> Result<(), EngineError> {
        if state.tick() == 0 {
            return self.expect_marker_zero(context, state);
        }

        if state.finalize_marker() == 0 {
            return Err(self.invariant_error(
                context,
                format!(
                    "finalize_marker missing for completed tick {}",
                    state.tick(),
                ),
            ));
        }

        Ok(())
    }

    fn expect_current_marker(
        &self,
        context: ValidationContext,
        state: &CounterState,
    ) -> Result<(), EngineError> {
        let expected_marker = state.preview_finalize_marker(self.seed, context.tick);
        if state.finalize_marker() != expected_marker {
            return Err(self.invariant_error(
                context,
                format!(
                    "finalize_marker mismatch for tick {}: expected {}, got {}",
                    context.tick,
                    expected_marker,
                    state.finalize_marker()
                ),
            ));
        }

        Ok(())
    }
}

impl StateValidator<CounterState> for CounterStateValidator {
    fn validate(
        &self,
        context: ValidationContext,
        state: &CounterState,
    ) -> Result<(), EngineError> {
        self.expect_tick_alignment(context, state)?;
        self.expect_limits(context, state)?;
        self.expect_entity_store(context, state)?;

        match context.checkpoint {
            ValidationCheckpoint::BeforeTickBegin => {
                self.expect_pending_cleared(context, state)?;
                self.expect_previous_marker(context, state)?;
            }
            ValidationCheckpoint::AfterInputApplied => {
                self.expect_marker_zero(context, state)?;
            }
            ValidationCheckpoint::AfterSimulationGroup => {
                self.expect_marker_zero(context, state)?;
            }
            ValidationCheckpoint::AfterFinalize => {
                self.expect_pending_cleared(context, state)?;
                self.expect_current_marker(context, state)?;
            }
        }

        Ok(())
    }
}

pub type CounterConfig = CounterSimulationConfig;
pub type CounterConfigArtifact = ConfigArtifact<CounterSimulationConfig>;
pub type CounterConfigArtifactCodec =
    ConfigArtifactSerializer<CounterSimulationConfig, CounterConfigTextSerializer>;
pub type CounterReplayLog = InMemoryReplayLog<CounterCommand, CounterSnapshot>;
pub type CounterScheduler =
    FixedScheduler<CounterState, CounterCommand, SplitMix64, CounterReplayLog>;
pub type CounterEngine = DeterministicEngine<
    CounterState,
    CounterCommand,
    SplitMix64,
    CounterReplayLog,
    CounterScheduler,
    CounterStateValidator,
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
pub type CounterScenario = SimulationScenario<CounterCommand, CounterSimulationConfig>;
pub type CounterScenarioCodec = SimulationScenarioSerializer<
    CounterCommand,
    CounterSimulationConfig,
    CounterCommandTextSerializer,
    CounterConfigTextSerializer,
>;
pub type CounterScenarioExecutionResult = ScenarioExecutionResult<CounterCommand, CounterSnapshot>;
pub type CounterScenarioVerificationResult =
    ScenarioVerificationResult<CounterCommand, CounterSnapshot>;
pub type CounterGoldenFixture =
    GoldenFixture<CounterCommand, CounterSnapshot, CounterSimulationConfig>;
pub type CounterGoldenFixtureResult = GoldenFixtureResult;
pub type CounterGoldenFixtureCodec = GoldenFixtureSerializer<
    CounterCommand,
    CounterSnapshot,
    CounterSimulationConfig,
    CounterCommandTextSerializer,
    CounterSnapshotTextSerializer,
    CounterConfigTextSerializer,
>;
pub type CounterParitySummary = ParityArtifactSummary;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterInteropArtifacts {
    pub config_artifact: Vec<u8>,
    pub scenario_artifact: Option<Vec<u8>>,
    pub replay_artifact: Vec<u8>,
    pub snapshot_artifact: Option<Vec<u8>>,
    pub golden_fixture: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterArtifactDigests {
    pub config_artifact: u64,
    pub scenario_artifact: Option<u64>,
    pub replay_artifact: u64,
    pub snapshot_artifact: Option<u64>,
    pub golden_fixture: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterScenarioInteropBundle {
    pub artifacts: CounterInteropArtifacts,
    pub digests: CounterArtifactDigests,
    pub summary: ArtifactSummary,
    pub parity_summary: CounterParitySummary,
    pub inspection: ReplayInspectionView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CounterFixtureInteropBundle {
    pub artifacts: CounterInteropArtifacts,
    pub digests: CounterArtifactDigests,
    pub summary: GoldenFixtureSummary,
    pub parity_summary: CounterParitySummary,
}

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
        ctx.state_mut().reset_finalize_marker();
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
        let delta = ctx.frame().command.delta;
        let consume_entropy = ctx.frame().command.consume_entropy;
        let entropy = if consume_entropy {
            let mut rng = ctx.rng_for(self.name());
            rng.next_u64() & 0xff
        } else {
            0
        };

        ctx.state_mut().stage_input(delta, entropy);
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
        ctx.state_mut().simulate();
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
        ctx.state_mut().settle();
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
        let marker = mix64(ctx.tick_seed() ^ ctx.state().checksum() ^ ctx.tick());
        ctx.state_mut().finalize(marker);
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

fn counter_config_from_policy(snapshot_policy: SnapshotPolicy) -> CounterSimulationConfig {
    CounterSimulationConfig {
        snapshot_policy,
        ..CounterSimulationConfig::default()
    }
}

fn build_counter_engine_internal(
    seed: Seed,
    scheduler: CounterScheduler,
    config: CounterSimulationConfig,
) -> CounterEngine {
    DeterministicEngine::new(
        seed,
        CounterState::with_initial_entities(
            config.initial_value,
            config.initial_velocity,
            &config.initial_entities,
        ),
        scheduler,
        CounterReplayLog::default(),
    )
    .with_snapshot_policy(config.snapshot_policy)
    .with_validator(
        CounterStateValidator::new(seed, config.clone()),
        config.validation_policy,
    )
}

pub fn default_counter_config() -> CounterSimulationConfig {
    CounterSimulationConfig::default()
}

pub fn counter_config_with_policy(snapshot_policy: SnapshotPolicy) -> CounterSimulationConfig {
    counter_config_from_policy(snapshot_policy)
}

pub fn counter_config_artifact_codec() -> CounterConfigArtifactCodec {
    ConfigArtifactSerializer::new(COUNTER_ENGINE_FAMILY, CounterConfigTextSerializer)
}

pub fn build_counter_config_artifact(
    config: &CounterSimulationConfig,
) -> Result<CounterConfigArtifact, EngineError> {
    config.validate()?;
    counter_config_artifact_codec().build_artifact(config)
}

pub fn export_counter_config_artifact(
    artifact: &CounterConfigArtifact,
) -> Result<Vec<u8>, EngineError> {
    counter_config_artifact_codec().encode(artifact)
}

pub fn counter_config_artifact_digest(
    artifact: &CounterConfigArtifact,
) -> Result<u64, EngineError> {
    counter_config_artifact_codec().digest(artifact)
}

pub fn import_counter_config_artifact(bytes: &[u8]) -> Result<CounterConfigArtifact, EngineError> {
    counter_config_artifact_codec().decode(bytes)
}

pub(crate) fn build_counter_engine(
    seed: Seed,
    scheduler: CounterScheduler,
    snapshot_policy: SnapshotPolicy,
) -> CounterEngine {
    build_counter_engine_internal(seed, scheduler, counter_config_from_policy(snapshot_policy))
}

pub fn counter_engine_with_config(
    seed: Seed,
    config: &CounterSimulationConfig,
) -> Result<CounterEngine, EngineError> {
    config.validate()?;
    Ok(build_counter_engine_internal(
        seed,
        default_counter_scheduler(),
        config.clone(),
    ))
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

pub fn counter_scenario_codec() -> CounterScenarioCodec {
    SimulationScenarioSerializer::new(
        CounterCommandTextSerializer,
        counter_config_artifact_codec(),
    )
}

pub fn counter_golden_fixture_codec() -> CounterGoldenFixtureCodec {
    GoldenFixtureSerializer::new(
        counter_replay_artifact_codec(),
        counter_snapshot_artifact_codec(),
        counter_config_artifact_codec(),
        counter_scenario_codec(),
    )
}

pub fn build_counter_scenario(
    config: &CounterSimulationConfig,
    seed: Seed,
    frames: &[InputFrame<CounterCommand>],
    expected_parity_summary: Option<CounterParitySummary>,
) -> Result<CounterScenario, EngineError> {
    let config_artifact = build_counter_config_artifact(config)?;
    Ok(counter_scenario_codec().build_scenario(
        config_artifact,
        seed,
        frames,
        expected_parity_summary,
    ))
}

pub fn export_counter_scenario(scenario: &CounterScenario) -> Result<Vec<u8>, EngineError> {
    counter_scenario_codec().encode(scenario)
}

pub fn import_counter_scenario(bytes: &[u8]) -> Result<CounterScenario, EngineError> {
    counter_scenario_codec().decode(bytes)
}

pub fn counter_scenario_digest(scenario: &CounterScenario) -> Result<u64, EngineError> {
    counter_scenario_codec().digest(scenario)
}

pub fn execute_counter_scenario(
    scenario: &CounterScenario,
) -> Result<CounterScenarioExecutionResult, EngineError> {
    counter_scenario_codec().execute::<CounterState, CounterEngine, _, _>(
        scenario,
        |seed, config| {
            build_counter_engine_internal(seed, default_counter_scheduler(), config.clone())
        },
        &counter_replay_artifact_codec(),
    )
}

pub fn verify_counter_scenario(
    scenario: &CounterScenario,
) -> Result<CounterScenarioVerificationResult, EngineError> {
    counter_scenario_codec().verify::<CounterState, CounterEngine, _, _>(
        scenario,
        |seed, config| {
            build_counter_engine_internal(seed, default_counter_scheduler(), config.clone())
        },
        &counter_replay_artifact_codec(),
    )
}

pub fn record_counter_replay_with_config(
    config: &CounterSimulationConfig,
    seed: Seed,
    frames: &[InputFrame<CounterCommand>],
) -> Result<CounterRecordedReplay, EngineError> {
    let config_artifact = build_counter_config_artifact(config)?;
    let codec = counter_replay_artifact_codec();
    let config = config.clone();
    record_replay::<CounterCommand, CounterState, CounterEngine, _, _, _>(
        seed,
        config.snapshot_policy,
        config_artifact.metadata.identity,
        frames,
        move |seed, _snapshot_policy| {
            build_counter_engine_internal(seed, default_counter_scheduler(), config.clone())
        },
        &codec,
    )
}

pub fn record_counter_replay(
    seed: Seed,
    snapshot_policy: SnapshotPolicy,
    frames: &[InputFrame<CounterCommand>],
) -> Result<CounterRecordedReplay, EngineError> {
    record_counter_replay_with_config(&counter_config_from_policy(snapshot_policy), seed, frames)
}

pub fn export_counter_replay_artifact(
    artifact: &CounterReplayArtifact,
) -> Result<Vec<u8>, EngineError> {
    counter_replay_artifact_codec().encode(artifact)
}

pub fn counter_replay_artifact_digest(
    artifact: &CounterReplayArtifact,
) -> Result<u64, EngineError> {
    counter_replay_artifact_codec().digest(artifact)
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

pub fn verify_counter_replay_with_config(
    config: &CounterSimulationConfig,
    artifact: &CounterReplayArtifact,
) -> Result<CounterReplayResult, EngineError> {
    let config_artifact = build_counter_config_artifact(config)?;
    let codec = counter_replay_artifact_codec();
    let config = config.clone();
    execute_replay_verify::<CounterCommand, CounterState, CounterEngine, _, _, _>(
        artifact,
        config_artifact.metadata.identity,
        move |seed, _snapshot_policy| {
            build_counter_engine_internal(seed, default_counter_scheduler(), config.clone())
        },
        &codec,
    )
}

pub fn verify_counter_replay(
    artifact: &CounterReplayArtifact,
) -> Result<CounterReplayResult, EngineError> {
    verify_counter_replay_with_config(
        &counter_config_from_policy(artifact.metadata.snapshot_policy),
        artifact,
    )
}

pub fn counter_snapshot_artifact_from_engine(
    config: &CounterSimulationConfig,
    engine: &CounterEngine,
) -> Result<CounterSnapshotArtifact, EngineError> {
    let config_artifact = build_counter_config_artifact(config)?;
    Ok(counter_snapshot_artifact_codec().build_artifact(
        engine.seed(),
        config_artifact.metadata.identity,
        &engine.manual_snapshot(),
    ))
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

    Ok(counter_snapshot_artifact_codec().build_artifact(
        artifact.metadata.base_seed,
        artifact.metadata.config_identity,
        snapshot,
    ))
}

pub fn export_counter_snapshot_artifact(
    artifact: &CounterSnapshotArtifact,
) -> Result<Vec<u8>, EngineError> {
    counter_snapshot_artifact_codec().encode(artifact)
}

pub fn counter_snapshot_artifact_digest(
    artifact: &CounterSnapshotArtifact,
) -> Result<u64, EngineError> {
    counter_snapshot_artifact_codec().digest(artifact)
}

pub fn import_counter_snapshot_artifact(
    bytes: &[u8],
) -> Result<CounterSnapshotArtifact, EngineError> {
    counter_snapshot_artifact_codec().decode(bytes)
}

pub fn generate_counter_golden_fixture_with_config(
    config: &CounterSimulationConfig,
    seed: Seed,
    frames: &[InputFrame<CounterCommand>],
) -> Result<CounterGoldenFixture, EngineError> {
    let config_artifact = build_counter_config_artifact(config)?;
    let config = config.clone();
    counter_golden_fixture_codec().generate_fixture::<CounterState, CounterEngine, _>(
        seed,
        config_artifact,
        frames,
        move |seed, _config| {
            build_counter_engine_internal(seed, default_counter_scheduler(), config.clone())
        },
    )
}

pub fn generate_counter_golden_fixture_from_scenario(
    scenario: &CounterScenario,
) -> Result<CounterGoldenFixture, EngineError> {
    counter_golden_fixture_codec().generate_fixture_from_scenario::<CounterState, CounterEngine, _>(
        scenario,
        |seed, config| {
            build_counter_engine_internal(seed, default_counter_scheduler(), config.clone())
        },
    )
}

pub fn generate_counter_golden_fixture(
    seed: Seed,
    snapshot_policy: SnapshotPolicy,
    frames: &[InputFrame<CounterCommand>],
) -> Result<CounterGoldenFixture, EngineError> {
    generate_counter_golden_fixture_with_config(
        &counter_config_from_policy(snapshot_policy),
        seed,
        frames,
    )
}

pub fn export_counter_golden_fixture(
    fixture: &CounterGoldenFixture,
) -> Result<Vec<u8>, EngineError> {
    counter_golden_fixture_codec().encode(fixture)
}

pub fn counter_golden_fixture_digest(fixture: &CounterGoldenFixture) -> Result<u64, EngineError> {
    counter_golden_fixture_codec().digest(fixture)
}

pub fn import_counter_golden_fixture(bytes: &[u8]) -> Result<CounterGoldenFixture, EngineError> {
    counter_golden_fixture_codec().decode(bytes)
}

pub fn verify_counter_golden_fixture(
    fixture: &CounterGoldenFixture,
) -> Result<CounterGoldenFixtureResult, EngineError> {
    counter_golden_fixture_codec().verify_fixture::<CounterState, CounterEngine, _>(
        fixture,
        |seed, config| {
            build_counter_engine_internal(seed, default_counter_scheduler(), config.clone())
        },
    )
}

pub fn execute_counter_scenario_interop_bundle(
    scenario: &CounterScenario,
) -> Result<CounterScenarioInteropBundle, EngineError> {
    let execution = execute_counter_scenario(scenario)?;
    Ok(CounterScenarioInteropBundle {
        artifacts: CounterInteropArtifacts {
            config_artifact: export_counter_config_artifact(&scenario.config_artifact)?,
            scenario_artifact: Some(export_counter_scenario(scenario)?),
            replay_artifact: export_counter_replay_artifact(&execution.replay.artifact)?,
            snapshot_artifact: execution
                .final_snapshot
                .as_ref()
                .map(export_counter_snapshot_artifact)
                .transpose()?,
            golden_fixture: None,
        },
        digests: CounterArtifactDigests {
            config_artifact: counter_config_artifact_digest(&scenario.config_artifact)?,
            scenario_artifact: Some(counter_scenario_digest(scenario)?),
            replay_artifact: counter_replay_artifact_digest(&execution.replay.artifact)?,
            snapshot_artifact: execution
                .final_snapshot
                .as_ref()
                .map(counter_snapshot_artifact_digest)
                .transpose()?,
            golden_fixture: None,
        },
        summary: execution.replay.result.summary.clone(),
        parity_summary: execution.parity_summary.clone(),
        inspection: execution.inspection,
    })
}

pub fn export_counter_fixture_interop_bundle(
    fixture: &CounterGoldenFixture,
) -> Result<CounterFixtureInteropBundle, EngineError> {
    Ok(CounterFixtureInteropBundle {
        artifacts: CounterInteropArtifacts {
            config_artifact: export_counter_config_artifact(&fixture.config_artifact)?,
            scenario_artifact: fixture
                .scenario_artifact
                .as_ref()
                .map(export_counter_scenario)
                .transpose()?,
            replay_artifact: export_counter_replay_artifact(&fixture.replay_artifact)?,
            snapshot_artifact: fixture
                .snapshot_artifact
                .as_ref()
                .map(export_counter_snapshot_artifact)
                .transpose()?,
            golden_fixture: Some(export_counter_golden_fixture(fixture)?),
        },
        digests: CounterArtifactDigests {
            config_artifact: counter_config_artifact_digest(&fixture.config_artifact)?,
            scenario_artifact: fixture
                .scenario_artifact
                .as_ref()
                .map(counter_scenario_digest)
                .transpose()?,
            replay_artifact: counter_replay_artifact_digest(&fixture.replay_artifact)?,
            snapshot_artifact: fixture
                .snapshot_artifact
                .as_ref()
                .map(counter_snapshot_artifact_digest)
                .transpose()?,
            golden_fixture: Some(counter_golden_fixture_digest(fixture)?),
        },
        summary: fixture.summary.clone(),
        parity_summary: CounterParitySummary::from(&fixture.summary),
    })
}

pub fn resume_counter_replay_from_snapshot_with_config(
    config: &CounterSimulationConfig,
    snapshot: &CounterSnapshotArtifact,
    artifact: &CounterReplayArtifact,
) -> Result<CounterReplayResult, EngineError> {
    let config_artifact = build_counter_config_artifact(config)?;
    let codec = counter_replay_artifact_codec();
    let config = config.clone();
    execute_replay_from_snapshot::<CounterCommand, CounterState, CounterEngine, _, _, _>(
        snapshot,
        artifact,
        config_artifact.metadata.identity,
        move |seed, _snapshot_policy| {
            build_counter_engine_internal(seed, default_counter_scheduler(), config.clone())
        },
        &codec,
    )
}

pub fn resume_counter_replay_from_snapshot(
    snapshot: &CounterSnapshotArtifact,
    artifact: &CounterReplayArtifact,
) -> Result<CounterReplayResult, EngineError> {
    resume_counter_replay_from_snapshot_with_config(
        &counter_config_from_policy(artifact.metadata.snapshot_policy),
        snapshot,
        artifact,
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
