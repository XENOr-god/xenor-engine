use crate::config::{ConfigArtifact, ConfigArtifactSerializer};
use crate::core::{EngineError, Seed, Tick};
use crate::engine::DeterministicEngine;
use crate::fixture::{
    GoldenFixture, GoldenFixtureResult, GoldenFixtureSerializer, GoldenFixtureSummary,
};
use crate::input::InputFrame;
use crate::parity::{ParityArtifactSummary, compare_parity_summaries};
use crate::persistence::{
    ArtifactSummary, RecordedReplay, ReplayArtifact, ReplayArtifactSerializer,
    ReplayExecutionResult, SnapshotArtifact, SnapshotArtifactSerializer,
    execute_replay_from_snapshot, execute_replay_verify, record_replay,
};
use crate::replay::{InMemoryReplayLog, ReplayInspectionView, inspect_replay_trace};
use crate::rng::SplitMix64;
use crate::scenario::{
    ScenarioExecutionResult, ScenarioVerificationResult, SimulationScenario,
    SimulationScenarioSerializer,
};
use crate::scheduler::{FixedScheduler, PhaseGroup};
use crate::serialization::{
    Serializer, SettlementCommandTextSerializer, SettlementConfigTextSerializer,
    SettlementSnapshotTextSerializer,
};
use crate::settlement::{
    ApplySettlementAllocationPhase, FinalizeSettlementPhase, ResetSettlementFinalizeMarkerPhase,
    ResolveSettlementStatusPhase, SettlementCommand, SettlementNamedScenario,
    SettlementProductionPhase, SettlementRunSummary, SettlementScenarioExpectation,
    SettlementSimulationConfig, SettlementSnapshot, SettlementState, SettlementStateValidator,
    StageSettlementCommandPhase, settlement_demo_scenarios,
};

pub const SETTLEMENT_ENGINE_FAMILY: &str = "xenor-engine-rust/settlement";

pub type SettlementConfig = SettlementSimulationConfig;
pub type SettlementConfigArtifact = ConfigArtifact<SettlementSimulationConfig>;
pub type SettlementConfigArtifactCodec =
    ConfigArtifactSerializer<SettlementSimulationConfig, SettlementConfigTextSerializer>;
pub type SettlementReplayLog = InMemoryReplayLog<SettlementCommand, SettlementSnapshot>;
pub type SettlementScheduler =
    FixedScheduler<SettlementState, SettlementCommand, SplitMix64, SettlementReplayLog>;
pub type SettlementEngine = DeterministicEngine<
    SettlementState,
    SettlementCommand,
    SplitMix64,
    SettlementReplayLog,
    SettlementScheduler,
    SettlementStateValidator,
>;
pub type SettlementReplayArtifact = ReplayArtifact<SettlementCommand, SettlementSnapshot>;
pub type SettlementSnapshotArtifact = SnapshotArtifact<SettlementSnapshot>;
pub type SettlementRecordedReplay = RecordedReplay<SettlementCommand, SettlementSnapshot>;
pub type SettlementReplayResult = ReplayExecutionResult<SettlementSnapshot>;
pub type SettlementReplayArtifactCodec = ReplayArtifactSerializer<
    SettlementCommand,
    SettlementSnapshot,
    SettlementCommandTextSerializer,
    SettlementSnapshotTextSerializer,
>;
pub type SettlementSnapshotArtifactCodec =
    SnapshotArtifactSerializer<SettlementSnapshot, SettlementSnapshotTextSerializer>;
pub type SettlementScenario = SimulationScenario<SettlementCommand, SettlementSimulationConfig>;
pub type SettlementScenarioCodec = SimulationScenarioSerializer<
    SettlementCommand,
    SettlementSimulationConfig,
    SettlementCommandTextSerializer,
    SettlementConfigTextSerializer,
>;
pub type SettlementScenarioExecutionResult =
    ScenarioExecutionResult<SettlementCommand, SettlementSnapshot>;
pub type SettlementScenarioVerificationResult =
    ScenarioVerificationResult<SettlementCommand, SettlementSnapshot>;
pub type SettlementGoldenFixture =
    GoldenFixture<SettlementCommand, SettlementSnapshot, SettlementSimulationConfig>;
pub type SettlementGoldenFixtureResult = GoldenFixtureResult;
pub type SettlementGoldenFixtureCodec = GoldenFixtureSerializer<
    SettlementCommand,
    SettlementSnapshot,
    SettlementSimulationConfig,
    SettlementCommandTextSerializer,
    SettlementSnapshotTextSerializer,
    SettlementConfigTextSerializer,
>;
pub type SettlementParitySummary = ParityArtifactSummary;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementScenarioCase {
    pub id: String,
    pub title: String,
    pub description: String,
    pub expectation: SettlementScenarioExpectation,
    pub scenario: SettlementScenario,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementExpectationCheck {
    pub passed: bool,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementInteropArtifacts {
    pub config_artifact: Vec<u8>,
    pub scenario_artifact: Option<Vec<u8>>,
    pub replay_artifact: Vec<u8>,
    pub snapshot_artifact: Option<Vec<u8>>,
    pub golden_fixture: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementArtifactDigests {
    pub config_artifact: u64,
    pub scenario_artifact: Option<u64>,
    pub replay_artifact: u64,
    pub snapshot_artifact: Option<u64>,
    pub golden_fixture: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementArtifactSizes {
    pub config_artifact: usize,
    pub scenario_artifact: Option<usize>,
    pub replay_artifact: usize,
    pub snapshot_artifact: Option<usize>,
    pub golden_fixture: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementScenarioInteropBundle {
    pub artifacts: SettlementInteropArtifacts,
    pub digests: SettlementArtifactDigests,
    pub sizes: SettlementArtifactSizes,
    pub summary: ArtifactSummary,
    pub parity_summary: SettlementParitySummary,
    pub inspection: ReplayInspectionView,
    pub run_summary: SettlementRunSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementFixtureInteropBundle {
    pub artifacts: SettlementInteropArtifacts,
    pub digests: SettlementArtifactDigests,
    pub sizes: SettlementArtifactSizes,
    pub summary: GoldenFixtureSummary,
    pub parity_summary: SettlementParitySummary,
    pub run_summary: SettlementRunSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementTickDigestView {
    pub tick: Tick,
    pub checksum: u64,
    pub snapshot_present: bool,
    pub phase_order: Vec<String>,
    pub validation_checkpoints: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementDeterminismReport {
    pub expectation_check: SettlementExpectationCheck,
    pub rerun_parity_match: bool,
    pub replay_verification_match: bool,
    pub replay_verification_checksum: u64,
    pub resume_snapshot_tick: Option<Tick>,
    pub resume_from_snapshot_match: bool,
    pub resume_from_snapshot_checksum: Option<u64>,
    pub fixture_verification_passed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementDemoScenarioView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub seed: Seed,
    pub tick_count: Tick,
    pub config: SettlementSimulationConfig,
    pub expected: SettlementScenarioExpectation,
    pub run_summary: SettlementRunSummary,
    pub artifact_summary: ArtifactSummary,
    pub parity_summary: SettlementParitySummary,
    pub artifact_digests: SettlementArtifactDigests,
    pub artifact_sizes: SettlementArtifactSizes,
    pub tick_digest_view: Vec<SettlementTickDigestView>,
    pub determinism: SettlementDeterminismReport,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementDemoCatalog {
    pub engine_family: String,
    pub scenario_count: usize,
    pub scenarios: Vec<SettlementDemoScenarioView>,
}

fn default_settlement_scheduler(config: &SettlementSimulationConfig) -> SettlementScheduler {
    let mut scheduler = SettlementScheduler::new();
    scheduler.add_phase(PhaseGroup::PreInput, ResetSettlementFinalizeMarkerPhase);
    scheduler.add_phase(PhaseGroup::Input, StageSettlementCommandPhase);
    scheduler.add_phase(PhaseGroup::Simulation, ApplySettlementAllocationPhase);
    scheduler.add_phase(
        PhaseGroup::Simulation,
        SettlementProductionPhase::new(config.clone()),
    );
    scheduler.add_phase(
        PhaseGroup::Simulation,
        crate::settlement::SettlementConsumptionPhase::new(config.clone()),
    );
    scheduler.add_phase(PhaseGroup::Simulation, ResolveSettlementStatusPhase);
    scheduler.add_phase(PhaseGroup::Finalize, FinalizeSettlementPhase);
    scheduler
}

fn build_settlement_engine_internal(
    seed: Seed,
    config: SettlementSimulationConfig,
) -> SettlementEngine {
    let snapshot_policy = config.snapshot_policy;
    let validation_policy = config.validation_policy;
    DeterministicEngine::new(
        seed,
        SettlementState::from_config(&config)
            .expect("validated settlement config should build authoritative state"),
        default_settlement_scheduler(&config),
        SettlementReplayLog::default(),
    )
    .with_snapshot_policy(snapshot_policy)
    .with_validator(
        SettlementStateValidator::new(seed, config.clone()),
        validation_policy,
    )
}

fn settlement_run_summary_from_snapshot(
    snapshot: &SettlementSnapshot,
) -> Result<SettlementRunSummary, EngineError> {
    SettlementRunSummary::from_snapshot(snapshot)
}

fn settlement_run_summary_from_execution(
    execution: &SettlementScenarioExecutionResult,
) -> Result<SettlementRunSummary, EngineError> {
    let snapshot = execution
        .final_snapshot
        .as_ref()
        .ok_or_else(|| EngineError::ReplayContinuationMismatch {
            detail:
                "settlement run summary requires a captured final snapshot; use a snapshot policy that records the final tick".into(),
        })?;
    settlement_run_summary_from_snapshot(&snapshot.payload)
}

fn settlement_expectation_check(
    expected: &SettlementScenarioExpectation,
    summary: &SettlementRunSummary,
) -> SettlementExpectationCheck {
    match expected.verify(summary) {
        Ok(()) => SettlementExpectationCheck {
            passed: true,
            detail: None,
        },
        Err(detail) => SettlementExpectationCheck {
            passed: false,
            detail: Some(detail),
        },
    }
}

fn settlement_tick_digest_view(inspection: &ReplayInspectionView) -> Vec<SettlementTickDigestView> {
    inspection
        .tick_summaries
        .iter()
        .map(|summary| SettlementTickDigestView {
            tick: summary.tick,
            checksum: summary.checksum,
            snapshot_present: summary.snapshot_present,
            phase_order: summary
                .phase_markers
                .iter()
                .map(|marker| marker.name.clone())
                .collect(),
            validation_checkpoints: summary
                .validation_summaries
                .iter()
                .map(|entry| entry.checkpoint.as_str().to_string())
                .collect(),
        })
        .collect()
}

fn scenario_interop_bundle_from_execution(
    scenario: &SettlementScenario,
    execution: &SettlementScenarioExecutionResult,
) -> Result<SettlementScenarioInteropBundle, EngineError> {
    let config_artifact = export_settlement_config_artifact(&scenario.config_artifact)?;
    let scenario_artifact = export_settlement_scenario(scenario)?;
    let replay_artifact = export_settlement_replay_artifact(&execution.replay.artifact)?;
    let snapshot_artifact = execution
        .final_snapshot
        .as_ref()
        .map(export_settlement_snapshot_artifact)
        .transpose()?;

    let artifacts = SettlementInteropArtifacts {
        config_artifact,
        scenario_artifact: Some(scenario_artifact),
        replay_artifact,
        snapshot_artifact,
        golden_fixture: None,
    };

    Ok(SettlementScenarioInteropBundle {
        digests: SettlementArtifactDigests {
            config_artifact: settlement_config_artifact_digest(&scenario.config_artifact)?,
            scenario_artifact: Some(settlement_scenario_digest(scenario)?),
            replay_artifact: settlement_replay_artifact_digest(&execution.replay.artifact)?,
            snapshot_artifact: execution
                .final_snapshot
                .as_ref()
                .map(settlement_snapshot_artifact_digest)
                .transpose()?,
            golden_fixture: None,
        },
        sizes: SettlementArtifactSizes {
            config_artifact: artifacts.config_artifact.len(),
            scenario_artifact: artifacts.scenario_artifact.as_ref().map(Vec::len),
            replay_artifact: artifacts.replay_artifact.len(),
            snapshot_artifact: artifacts.snapshot_artifact.as_ref().map(Vec::len),
            golden_fixture: None,
        },
        artifacts,
        summary: execution.replay.result.summary.clone(),
        parity_summary: execution.parity_summary.clone(),
        inspection: execution.inspection.clone(),
        run_summary: settlement_run_summary_from_execution(execution)?,
    })
}

fn fixture_interop_bundle_from_fixture(
    fixture: &SettlementGoldenFixture,
) -> Result<SettlementFixtureInteropBundle, EngineError> {
    let config_artifact = export_settlement_config_artifact(&fixture.config_artifact)?;
    let scenario_artifact = fixture
        .scenario_artifact
        .as_ref()
        .map(export_settlement_scenario)
        .transpose()?;
    let replay_artifact = export_settlement_replay_artifact(&fixture.replay_artifact)?;
    let snapshot_artifact = fixture
        .snapshot_artifact
        .as_ref()
        .map(export_settlement_snapshot_artifact)
        .transpose()?;
    let golden_fixture = export_settlement_golden_fixture(fixture)?;

    let artifacts = SettlementInteropArtifacts {
        config_artifact,
        scenario_artifact,
        replay_artifact,
        snapshot_artifact,
        golden_fixture: Some(golden_fixture),
    };

    let run_summary = settlement_run_summary_from_snapshot(
        &fixture
            .snapshot_artifact
            .as_ref()
            .ok_or_else(|| EngineError::ReplayContinuationMismatch {
                detail: "settlement fixture interop bundle requires a captured final snapshot"
                    .into(),
            })?
            .payload,
    )?;

    Ok(SettlementFixtureInteropBundle {
        digests: SettlementArtifactDigests {
            config_artifact: settlement_config_artifact_digest(&fixture.config_artifact)?,
            scenario_artifact: fixture
                .scenario_artifact
                .as_ref()
                .map(settlement_scenario_digest)
                .transpose()?,
            replay_artifact: settlement_replay_artifact_digest(&fixture.replay_artifact)?,
            snapshot_artifact: fixture
                .snapshot_artifact
                .as_ref()
                .map(settlement_snapshot_artifact_digest)
                .transpose()?,
            golden_fixture: Some(settlement_golden_fixture_digest(fixture)?),
        },
        sizes: SettlementArtifactSizes {
            config_artifact: artifacts.config_artifact.len(),
            scenario_artifact: artifacts.scenario_artifact.as_ref().map(Vec::len),
            replay_artifact: artifacts.replay_artifact.len(),
            snapshot_artifact: artifacts.snapshot_artifact.as_ref().map(Vec::len),
            golden_fixture: artifacts.golden_fixture.as_ref().map(Vec::len),
        },
        artifacts,
        summary: fixture.summary.clone(),
        parity_summary: SettlementParitySummary::from(&fixture.summary),
        run_summary,
    })
}

pub fn default_settlement_config() -> SettlementSimulationConfig {
    SettlementSimulationConfig::default()
}

pub fn settlement_config_artifact_codec() -> SettlementConfigArtifactCodec {
    ConfigArtifactSerializer::new(SETTLEMENT_ENGINE_FAMILY, SettlementConfigTextSerializer)
}

pub fn settlement_replay_artifact_codec() -> SettlementReplayArtifactCodec {
    ReplayArtifactSerializer::new(
        SETTLEMENT_ENGINE_FAMILY,
        SettlementCommandTextSerializer,
        SettlementSnapshotTextSerializer,
    )
}

pub fn settlement_snapshot_artifact_codec() -> SettlementSnapshotArtifactCodec {
    SnapshotArtifactSerializer::new(SETTLEMENT_ENGINE_FAMILY, SettlementSnapshotTextSerializer)
}

pub fn settlement_scenario_codec() -> SettlementScenarioCodec {
    SimulationScenarioSerializer::new(
        SettlementCommandTextSerializer,
        settlement_config_artifact_codec(),
    )
}

pub fn settlement_golden_fixture_codec() -> SettlementGoldenFixtureCodec {
    GoldenFixtureSerializer::new(
        settlement_replay_artifact_codec(),
        settlement_snapshot_artifact_codec(),
        settlement_config_artifact_codec(),
        settlement_scenario_codec(),
    )
}

pub fn build_settlement_config_artifact(
    config: &SettlementSimulationConfig,
) -> Result<SettlementConfigArtifact, EngineError> {
    config.validate()?;
    settlement_config_artifact_codec().build_artifact(config)
}

pub fn export_settlement_config_artifact(
    artifact: &SettlementConfigArtifact,
) -> Result<Vec<u8>, EngineError> {
    settlement_config_artifact_codec().encode(artifact)
}

pub fn settlement_config_artifact_digest(
    artifact: &SettlementConfigArtifact,
) -> Result<u64, EngineError> {
    settlement_config_artifact_codec().digest(artifact)
}

pub fn import_settlement_config_artifact(
    bytes: &[u8],
) -> Result<SettlementConfigArtifact, EngineError> {
    settlement_config_artifact_codec().decode(bytes)
}

pub fn settlement_engine_with_config(
    seed: Seed,
    config: &SettlementSimulationConfig,
) -> Result<SettlementEngine, EngineError> {
    config.validate()?;
    Ok(build_settlement_engine_internal(seed, config.clone()))
}

pub fn build_settlement_scenario(
    config: &SettlementSimulationConfig,
    seed: Seed,
    frames: &[InputFrame<SettlementCommand>],
    expected_parity_summary: Option<SettlementParitySummary>,
) -> Result<SettlementScenario, EngineError> {
    let config_artifact = build_settlement_config_artifact(config)?;
    Ok(settlement_scenario_codec().build_scenario(
        config_artifact,
        seed,
        frames,
        expected_parity_summary,
    ))
}

pub fn export_settlement_scenario(scenario: &SettlementScenario) -> Result<Vec<u8>, EngineError> {
    settlement_scenario_codec().encode(scenario)
}

pub fn import_settlement_scenario(bytes: &[u8]) -> Result<SettlementScenario, EngineError> {
    settlement_scenario_codec().decode(bytes)
}

pub fn settlement_scenario_digest(scenario: &SettlementScenario) -> Result<u64, EngineError> {
    settlement_scenario_codec().digest(scenario)
}

pub fn execute_settlement_scenario(
    scenario: &SettlementScenario,
) -> Result<SettlementScenarioExecutionResult, EngineError> {
    settlement_scenario_codec().execute::<SettlementState, SettlementEngine, _, _>(
        scenario,
        |seed, config| build_settlement_engine_internal(seed, config.clone()),
        &settlement_replay_artifact_codec(),
    )
}

pub fn verify_settlement_scenario(
    scenario: &SettlementScenario,
) -> Result<SettlementScenarioVerificationResult, EngineError> {
    settlement_scenario_codec().verify::<SettlementState, SettlementEngine, _, _>(
        scenario,
        |seed, config| build_settlement_engine_internal(seed, config.clone()),
        &settlement_replay_artifact_codec(),
    )
}

pub fn record_settlement_replay_with_config(
    config: &SettlementSimulationConfig,
    seed: Seed,
    frames: &[InputFrame<SettlementCommand>],
) -> Result<SettlementRecordedReplay, EngineError> {
    let config_artifact = build_settlement_config_artifact(config)?;
    let config = config.clone();
    record_replay::<SettlementCommand, SettlementState, SettlementEngine, _, _, _>(
        seed,
        config.snapshot_policy,
        config_artifact.metadata.identity,
        frames,
        move |seed, _snapshot_policy| build_settlement_engine_internal(seed, config.clone()),
        &settlement_replay_artifact_codec(),
    )
}

pub fn export_settlement_replay_artifact(
    artifact: &SettlementReplayArtifact,
) -> Result<Vec<u8>, EngineError> {
    settlement_replay_artifact_codec().encode(artifact)
}

pub fn settlement_replay_artifact_digest(
    artifact: &SettlementReplayArtifact,
) -> Result<u64, EngineError> {
    settlement_replay_artifact_codec().digest(artifact)
}

pub fn import_settlement_replay_artifact(
    bytes: &[u8],
) -> Result<SettlementReplayArtifact, EngineError> {
    settlement_replay_artifact_codec().decode(bytes)
}

pub fn settlement_replay_summary(
    artifact: &SettlementReplayArtifact,
) -> Result<ArtifactSummary, EngineError> {
    settlement_replay_artifact_codec().summary(artifact)
}

pub fn settlement_parity_summary(
    artifact: &SettlementReplayArtifact,
) -> Result<SettlementParitySummary, EngineError> {
    settlement_replay_summary(artifact).map(|summary| SettlementParitySummary::from(&summary))
}

pub fn inspect_settlement_replay(artifact: &SettlementReplayArtifact) -> ReplayInspectionView {
    inspect_replay_trace(artifact.records.as_slice())
}

pub fn verify_settlement_replay_with_config(
    config: &SettlementSimulationConfig,
    artifact: &SettlementReplayArtifact,
) -> Result<SettlementReplayResult, EngineError> {
    let config_artifact = build_settlement_config_artifact(config)?;
    let config = config.clone();
    execute_replay_verify::<SettlementCommand, SettlementState, SettlementEngine, _, _, _>(
        artifact,
        config_artifact.metadata.identity,
        move |seed, _snapshot_policy| build_settlement_engine_internal(seed, config.clone()),
        &settlement_replay_artifact_codec(),
    )
}

pub fn settlement_snapshot_artifact_at_tick(
    artifact: &SettlementReplayArtifact,
    tick: Tick,
) -> Result<SettlementSnapshotArtifact, EngineError> {
    let snapshot = artifact
        .records
        .iter()
        .find(|record| record.tick == tick)
        .and_then(|record| record.snapshot.as_ref())
        .ok_or_else(|| EngineError::ReplayContinuationMismatch {
            detail: format!("no snapshot recorded at tick {tick}"),
        })?;

    Ok(settlement_snapshot_artifact_codec().build_artifact(
        artifact.metadata.base_seed,
        artifact.metadata.config_identity,
        snapshot,
    ))
}

pub fn export_settlement_snapshot_artifact(
    artifact: &SettlementSnapshotArtifact,
) -> Result<Vec<u8>, EngineError> {
    settlement_snapshot_artifact_codec().encode(artifact)
}

pub fn settlement_snapshot_artifact_digest(
    artifact: &SettlementSnapshotArtifact,
) -> Result<u64, EngineError> {
    settlement_snapshot_artifact_codec().digest(artifact)
}

pub fn import_settlement_snapshot_artifact(
    bytes: &[u8],
) -> Result<SettlementSnapshotArtifact, EngineError> {
    settlement_snapshot_artifact_codec().decode(bytes)
}

pub fn resume_settlement_replay_from_snapshot_with_config(
    config: &SettlementSimulationConfig,
    snapshot: &SettlementSnapshotArtifact,
    artifact: &SettlementReplayArtifact,
) -> Result<SettlementReplayResult, EngineError> {
    let config_artifact = build_settlement_config_artifact(config)?;
    let config = config.clone();
    execute_replay_from_snapshot::<SettlementCommand, SettlementState, SettlementEngine, _, _, _>(
        snapshot,
        artifact,
        config_artifact.metadata.identity,
        move |seed, _snapshot_policy| build_settlement_engine_internal(seed, config.clone()),
        &settlement_replay_artifact_codec(),
    )
}

pub fn generate_settlement_golden_fixture_from_scenario(
    scenario: &SettlementScenario,
) -> Result<SettlementGoldenFixture, EngineError> {
    settlement_golden_fixture_codec()
        .generate_fixture_from_scenario::<SettlementState, SettlementEngine, _>(
            scenario,
            |seed, config| build_settlement_engine_internal(seed, config.clone()),
        )
}

pub fn export_settlement_golden_fixture(
    fixture: &SettlementGoldenFixture,
) -> Result<Vec<u8>, EngineError> {
    settlement_golden_fixture_codec().encode(fixture)
}

pub fn settlement_golden_fixture_digest(
    fixture: &SettlementGoldenFixture,
) -> Result<u64, EngineError> {
    settlement_golden_fixture_codec().digest(fixture)
}

pub fn import_settlement_golden_fixture(
    bytes: &[u8],
) -> Result<SettlementGoldenFixture, EngineError> {
    settlement_golden_fixture_codec().decode(bytes)
}

pub fn verify_settlement_golden_fixture(
    fixture: &SettlementGoldenFixture,
) -> Result<SettlementGoldenFixtureResult, EngineError> {
    settlement_golden_fixture_codec()
        .verify_fixture::<SettlementState, SettlementEngine, _>(fixture, |seed, config| {
            build_settlement_engine_internal(seed, config.clone())
        })
}

pub fn build_settlement_case(
    named: &SettlementNamedScenario,
) -> Result<SettlementScenarioCase, EngineError> {
    Ok(SettlementScenarioCase {
        id: named.id.to_string(),
        title: named.title.to_string(),
        description: named.description.to_string(),
        expectation: named.expected.clone(),
        scenario: build_settlement_scenario(
            &named.config,
            named.seed,
            &named.input_frames(),
            None,
        )?,
    })
}

pub fn settlement_demo_cases() -> Result<Vec<SettlementScenarioCase>, EngineError> {
    settlement_demo_scenarios()
        .iter()
        .map(build_settlement_case)
        .collect()
}

pub fn execute_settlement_scenario_interop_bundle(
    scenario: &SettlementScenario,
) -> Result<SettlementScenarioInteropBundle, EngineError> {
    let execution = execute_settlement_scenario(scenario)?;
    scenario_interop_bundle_from_execution(scenario, &execution)
}

pub fn export_settlement_fixture_interop_bundle(
    fixture: &SettlementGoldenFixture,
) -> Result<SettlementFixtureInteropBundle, EngineError> {
    fixture_interop_bundle_from_fixture(fixture)
}

pub fn build_settlement_demo_catalog() -> Result<SettlementDemoCatalog, EngineError> {
    let mut scenarios = Vec::new();

    for case in settlement_demo_cases()? {
        let execution = execute_settlement_scenario(&case.scenario)?;
        let bundle = scenario_interop_bundle_from_execution(&case.scenario, &execution)?;
        let rerun = execute_settlement_scenario(&case.scenario)?;
        let rerun_parity_match =
            compare_parity_summaries(&bundle.parity_summary, &rerun.parity_summary).is_match();
        let replay_verify = verify_settlement_replay_with_config(
            &case.scenario.config_artifact.payload,
            &execution.replay.artifact,
        )?;

        let resume_snapshot = execution
            .replay
            .artifact
            .records
            .iter()
            .find_map(|record| record.snapshot.as_ref().map(|_| record.tick))
            .map(|tick| settlement_snapshot_artifact_at_tick(&execution.replay.artifact, tick))
            .transpose()?;
        let resume_snapshot_tick = resume_snapshot
            .as_ref()
            .map(|snapshot| snapshot.metadata.snapshot.source_tick);
        let resume_result = resume_snapshot
            .as_ref()
            .map(|snapshot| {
                resume_settlement_replay_from_snapshot_with_config(
                    &case.scenario.config_artifact.payload,
                    snapshot,
                    &execution.replay.artifact,
                )
            })
            .transpose()?;

        let fixture = generate_settlement_golden_fixture_from_scenario(&case.scenario)?;
        let fixture_result = verify_settlement_golden_fixture(&fixture)?;
        let fixture_bundle = fixture_interop_bundle_from_fixture(&fixture)?;

        let expectation_check =
            settlement_expectation_check(&case.expectation, &bundle.run_summary);
        if !expectation_check.passed {
            return Err(EngineError::SummaryMismatch {
                detail: expectation_check
                    .detail
                    .clone()
                    .unwrap_or_else(|| "settlement expectation mismatch".into()),
            });
        }

        scenarios.push(SettlementDemoScenarioView {
            id: case.id,
            title: case.title,
            description: case.description,
            seed: case.scenario.base_seed,
            tick_count: case.scenario.frames.len() as Tick,
            config: case.scenario.config_artifact.payload.clone(),
            expected: case.expectation.clone(),
            run_summary: bundle.run_summary.clone(),
            artifact_summary: bundle.summary.clone(),
            parity_summary: bundle.parity_summary.clone(),
            artifact_digests: fixture_bundle.digests,
            artifact_sizes: fixture_bundle.sizes,
            tick_digest_view: settlement_tick_digest_view(&bundle.inspection),
            determinism: SettlementDeterminismReport {
                expectation_check,
                rerun_parity_match,
                replay_verification_match: replay_verify.final_checksum
                    == bundle.summary.final_checksum,
                replay_verification_checksum: replay_verify.final_checksum,
                resume_snapshot_tick,
                resume_from_snapshot_match: resume_result
                    .as_ref()
                    .map(|result| result.final_checksum == bundle.summary.final_checksum)
                    .unwrap_or(false),
                resume_from_snapshot_checksum: resume_result
                    .as_ref()
                    .map(|result| result.final_checksum),
                fixture_verification_passed: fixture_result.passed(),
            },
        });
    }

    Ok(SettlementDemoCatalog {
        engine_family: SETTLEMENT_ENGINE_FAMILY.to_string(),
        scenario_count: scenarios.len(),
        scenarios,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        SETTLEMENT_ENGINE_FAMILY, build_settlement_case, build_settlement_demo_catalog,
        export_settlement_config_artifact, export_settlement_golden_fixture,
        export_settlement_replay_artifact, export_settlement_scenario,
        export_settlement_snapshot_artifact, generate_settlement_golden_fixture_from_scenario,
        import_settlement_config_artifact, import_settlement_golden_fixture,
        import_settlement_replay_artifact, import_settlement_scenario,
        import_settlement_snapshot_artifact, settlement_demo_cases, settlement_demo_scenarios,
        settlement_replay_summary, settlement_scenario_digest,
        settlement_snapshot_artifact_at_tick, verify_settlement_replay_with_config,
    };
    use crate::parity::compare_parity_summaries;
    use crate::settlement::{SettlementCommand, SettlementStatus, WorkerAllocation};

    #[test]
    fn settlement_vertical_slice_rerun_is_identical() {
        let case =
            build_settlement_case(&settlement_demo_scenarios()[0]).expect("demo case should build");
        let first = super::execute_settlement_scenario(&case.scenario)
            .expect("first scenario run should succeed");
        let second = super::execute_settlement_scenario(&case.scenario)
            .expect("second scenario run should succeed");

        assert!(compare_parity_summaries(&first.parity_summary, &second.parity_summary).is_match());
    }

    #[test]
    fn settlement_replay_roundtrip_is_identical() {
        let case =
            build_settlement_case(&settlement_demo_scenarios()[0]).expect("demo case should build");
        let execution = super::execute_settlement_scenario(&case.scenario)
            .expect("scenario run should succeed");

        let replay_bytes = export_settlement_replay_artifact(&execution.replay.artifact)
            .expect("replay artifact should export");
        let replay = import_settlement_replay_artifact(&replay_bytes)
            .expect("replay artifact should import");

        assert_eq!(replay, execution.replay.artifact);
        assert_eq!(
            settlement_replay_summary(&replay)
                .expect("replay summary should work")
                .final_checksum,
            execution.replay.result.summary.final_checksum,
        );
    }

    #[test]
    fn settlement_resume_from_snapshot_matches_full_run() {
        let case =
            build_settlement_case(&settlement_demo_scenarios()[3]).expect("demo case should build");
        let execution = super::execute_settlement_scenario(&case.scenario)
            .expect("scenario run should succeed");
        let snapshot = settlement_snapshot_artifact_at_tick(&execution.replay.artifact, 2)
            .expect("tick 2 snapshot should exist");
        let resumed = super::resume_settlement_replay_from_snapshot_with_config(
            &case.scenario.config_artifact.payload,
            &snapshot,
            &execution.replay.artifact,
        )
        .expect("resume from snapshot should succeed");

        assert_eq!(
            resumed.final_checksum,
            execution.replay.result.summary.final_checksum
        );
    }

    #[test]
    fn settlement_worker_reallocation_changes_outcome_deterministically() {
        let cases = settlement_demo_cases().expect("demo cases should build");
        let shortage = super::execute_settlement_scenario(&cases[1].scenario)
            .expect("shortage scenario should run");
        let recovery = super::execute_settlement_scenario(&cases[3].scenario)
            .expect("recovery scenario should run");
        let shortage_summary = super::settlement_run_summary_from_execution(&shortage)
            .expect("shortage summary should derive");
        let recovery_summary = super::settlement_run_summary_from_execution(&recovery)
            .expect("recovery summary should derive");

        assert_ne!(shortage_summary.final_food, recovery_summary.final_food);
        assert_eq!(recovery_summary.last_status, SettlementStatus::Stable);
        assert_eq!(
            recovery_summary.final_allocation,
            WorkerAllocation {
                farmers: 3,
                loggers: 1,
            }
        );
    }

    #[test]
    fn settlement_shortage_scenario_triggers_expected_condition() {
        let case = build_settlement_case(&settlement_demo_scenarios()[1])
            .expect("shortage case should build");
        let execution =
            super::execute_settlement_scenario(&case.scenario).expect("scenario should run");
        let summary = super::settlement_run_summary_from_execution(&execution)
            .expect("summary should derive");

        assert_eq!(summary.last_status, SettlementStatus::FoodShortage);
        assert_eq!(summary.shortage_ticks, 6);
        assert_eq!(summary.total_food_shortage, 24);
    }

    #[test]
    fn settlement_scenario_summaries_are_stable() {
        let catalog = build_settlement_demo_catalog().expect("catalog should build");
        let balanced = catalog
            .scenarios
            .iter()
            .find(|scenario| scenario.id == "balanced_settlement")
            .expect("balanced scenario should exist");

        assert_eq!(balanced.run_summary.final_food, 36);
        assert_eq!(balanced.run_summary.final_wood, 22);
        assert!(balanced.determinism.rerun_parity_match);
        assert!(balanced.determinism.fixture_verification_passed);
    }

    #[test]
    fn settlement_site_consumed_output_shape_matches_contract() {
        let catalog = build_settlement_demo_catalog().expect("catalog should build");

        assert_eq!(catalog.engine_family, SETTLEMENT_ENGINE_FAMILY);
        assert_eq!(catalog.scenario_count, 4);
        assert!(
            catalog
                .scenarios
                .iter()
                .all(|scenario| scenario.artifact_digests.golden_fixture.is_some())
        );
        assert!(
            catalog
                .scenarios
                .iter()
                .all(|scenario| !scenario.tick_digest_view.is_empty())
        );
    }

    #[test]
    fn settlement_config_roundtrip_is_canonical() {
        let case =
            build_settlement_case(&settlement_demo_scenarios()[0]).expect("demo case should build");
        let bytes = export_settlement_config_artifact(&case.scenario.config_artifact)
            .expect("config should export");
        let imported = import_settlement_config_artifact(&bytes).expect("config should import");
        let reexported =
            export_settlement_config_artifact(&imported).expect("config should reexport");

        assert_eq!(bytes, reexported);
    }

    #[test]
    fn settlement_scenario_roundtrip_is_canonical() {
        let case =
            build_settlement_case(&settlement_demo_scenarios()[0]).expect("demo case should build");
        let bytes = export_settlement_scenario(&case.scenario).expect("scenario should export");
        let imported = import_settlement_scenario(&bytes).expect("scenario should import");
        let reexported = export_settlement_scenario(&imported).expect("scenario should reexport");

        assert_eq!(bytes, reexported);
        assert_eq!(
            settlement_scenario_digest(&imported).expect("scenario digest should work"),
            settlement_scenario_digest(&case.scenario).expect("scenario digest should work"),
        );
    }

    #[test]
    fn settlement_snapshot_roundtrip_is_canonical() {
        let case =
            build_settlement_case(&settlement_demo_scenarios()[0]).expect("demo case should build");
        let execution = super::execute_settlement_scenario(&case.scenario)
            .expect("scenario run should succeed");
        let snapshot = settlement_snapshot_artifact_at_tick(&execution.replay.artifact, 2)
            .expect("tick 2 snapshot should exist");
        let bytes = export_settlement_snapshot_artifact(&snapshot).expect("snapshot should export");
        let imported = import_settlement_snapshot_artifact(&bytes).expect("snapshot should import");
        let reexported =
            export_settlement_snapshot_artifact(&imported).expect("snapshot should reexport");

        assert_eq!(bytes, reexported);
    }

    #[test]
    fn settlement_golden_fixture_roundtrip_is_canonical() {
        let case =
            build_settlement_case(&settlement_demo_scenarios()[2]).expect("demo case should build");
        let fixture = generate_settlement_golden_fixture_from_scenario(&case.scenario)
            .expect("fixture should generate");
        let bytes = export_settlement_golden_fixture(&fixture).expect("fixture should export");
        let imported = import_settlement_golden_fixture(&bytes).expect("fixture should import");
        let reexported =
            export_settlement_golden_fixture(&imported).expect("fixture should reexport");

        assert_eq!(bytes, reexported);
    }

    #[test]
    fn settlement_replay_verification_matches_configured_run() {
        let case =
            build_settlement_case(&settlement_demo_scenarios()[3]).expect("demo case should build");
        let execution = super::execute_settlement_scenario(&case.scenario)
            .expect("scenario run should succeed");
        let verified = verify_settlement_replay_with_config(
            &case.scenario.config_artifact.payload,
            &execution.replay.artifact,
        )
        .expect("replay verification should succeed");

        assert_eq!(
            verified.final_checksum,
            execution.replay.result.summary.final_checksum
        );
    }

    #[test]
    fn settlement_cases_include_real_commands() {
        let recovery = settlement_demo_scenarios()
            .into_iter()
            .find(|scenario| scenario.id == "recovery_after_reallocation")
            .expect("recovery scenario should exist");

        assert!(matches!(
            recovery.commands[2],
            SettlementCommand::SetWorkerAllocation(_)
        ));
    }
}
