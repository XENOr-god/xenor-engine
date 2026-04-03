use std::fmt;

use crate::canonical::{
    CANONICAL_TEXT_ENCODING, CanonicalLineReader, CanonicalLineWriter, canonical_digest,
    decode_hex, decode_hex_string, encode_hex,
};
use crate::config::{ConfigArtifact, ConfigArtifactSerializer, SimulationConfig};
use crate::core::{EngineError, Seed, Tick};
use crate::engine::ReplayableEngine;
use crate::input::{Command, InputFrame};
use crate::parity::{ParityArtifactSummary, ParityComparison, compare_parity_summaries};
use crate::persistence::{
    ArtifactSummary, ReplayArtifact, ReplayArtifactSerializer, SnapshotArtifact,
    SnapshotArtifactSerializer, record_replay,
};
use crate::replay::compare_replay_traces_with_snapshot_digest;
use crate::scenario::{SimulationScenario, SimulationScenarioSerializer};
use crate::serialization::Serializer;
use crate::state::SimulationState;

pub const GOLDEN_FIXTURE_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenFixtureMetadata {
    pub artifact_schema_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenFixtureSummary {
    pub engine_family: String,
    pub base_seed: Seed,
    pub final_tick: Tick,
    pub final_checksum: u64,
    pub config_payload_schema_version: u32,
    pub config_digest: u64,
    pub replay_artifact_schema_version: u32,
    pub snapshot_artifact_schema_version: u32,
    pub command_payload_schema_version: u32,
    pub snapshot_payload_schema_version: u32,
    pub replay_digest: u64,
    pub snapshot_digest: Option<u64>,
    pub scenario_digest: Option<u64>,
}

impl From<&ArtifactSummary> for GoldenFixtureSummary {
    fn from(value: &ArtifactSummary) -> Self {
        Self {
            engine_family: value.engine_family.clone(),
            base_seed: value.base_seed,
            final_tick: value.final_tick,
            final_checksum: value.final_checksum,
            config_payload_schema_version: value.config_payload_schema_version,
            config_digest: value.config_digest,
            replay_artifact_schema_version: value.replay_artifact_schema_version,
            snapshot_artifact_schema_version: value.snapshot_artifact_schema_version,
            command_payload_schema_version: value.command_payload_schema_version,
            snapshot_payload_schema_version: value.snapshot_payload_schema_version,
            replay_digest: value.replay_digest,
            snapshot_digest: value.snapshot_digest,
            scenario_digest: value.scenario_digest,
        }
    }
}

impl From<&GoldenFixtureSummary> for ParityArtifactSummary {
    fn from(value: &GoldenFixtureSummary) -> Self {
        Self {
            engine_family: value.engine_family.clone(),
            base_seed: value.base_seed,
            final_tick: value.final_tick,
            final_checksum: value.final_checksum,
            config_payload_schema_version: value.config_payload_schema_version,
            config_digest: value.config_digest,
            replay_artifact_schema_version: value.replay_artifact_schema_version,
            snapshot_artifact_schema_version: value.snapshot_artifact_schema_version,
            command_payload_schema_version: value.command_payload_schema_version,
            snapshot_payload_schema_version: value.snapshot_payload_schema_version,
            replay_digest: value.replay_digest,
            snapshot_digest: value.snapshot_digest,
            scenario_digest: value.scenario_digest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenFixture<C, Snapshot, Config>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
    Config: Clone + Eq + fmt::Debug,
{
    pub metadata: GoldenFixtureMetadata,
    pub config_artifact: ConfigArtifact<Config>,
    pub scenario_artifact: Option<SimulationScenario<C, Config>>,
    pub replay_artifact: ReplayArtifact<C, Snapshot>,
    pub snapshot_artifact: Option<SnapshotArtifact<Snapshot>>,
    pub summary: GoldenFixtureSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenFixtureResult {
    pub fixture_summary: GoldenFixtureSummary,
    pub actual_summary: ParityArtifactSummary,
    pub comparison: ParityComparison,
    pub config_mismatch: Option<String>,
    pub scenario_mismatch: Option<String>,
    pub replay_mismatch: Option<String>,
    pub snapshot_mismatch: Option<String>,
    pub summary_mismatch: Option<String>,
}

impl GoldenFixtureResult {
    pub fn passed(&self) -> bool {
        self.config_mismatch.is_none()
            && self.scenario_mismatch.is_none()
            && self.replay_mismatch.is_none()
            && self.snapshot_mismatch.is_none()
            && self.summary_mismatch.is_none()
            && self.comparison.is_match()
    }
}

#[derive(Clone, Debug)]
pub struct GoldenFixtureSerializer<
    C,
    Snapshot,
    Config,
    CommandSerializer,
    SnapshotSerializer,
    ConfigSerializer,
> where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
    Config: Clone + Eq + fmt::Debug,
{
    replay_serializer: ReplayArtifactSerializer<C, Snapshot, CommandSerializer, SnapshotSerializer>,
    snapshot_serializer: SnapshotArtifactSerializer<Snapshot, SnapshotSerializer>,
    config_artifact_serializer: ConfigArtifactSerializer<Config, ConfigSerializer>,
    scenario_serializer:
        SimulationScenarioSerializer<C, Config, CommandSerializer, ConfigSerializer>,
}

impl<C, Snapshot, Config, CommandSerializer, SnapshotSerializer, ConfigSerializer>
    GoldenFixtureSerializer<
        C,
        Snapshot,
        Config,
        CommandSerializer,
        SnapshotSerializer,
        ConfigSerializer,
    >
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
    Config: Clone + Eq + fmt::Debug + SimulationConfig,
    CommandSerializer: Serializer<C>,
    SnapshotSerializer: Serializer<Snapshot>,
    ConfigSerializer: Serializer<Config>,
    CommandSerializer::Error: fmt::Display,
    SnapshotSerializer::Error: fmt::Display,
    ConfigSerializer::Error: fmt::Display,
{
    pub fn new(
        replay_serializer: ReplayArtifactSerializer<
            C,
            Snapshot,
            CommandSerializer,
            SnapshotSerializer,
        >,
        snapshot_serializer: SnapshotArtifactSerializer<Snapshot, SnapshotSerializer>,
        config_artifact_serializer: ConfigArtifactSerializer<Config, ConfigSerializer>,
        scenario_serializer: SimulationScenarioSerializer<
            C,
            Config,
            CommandSerializer,
            ConfigSerializer,
        >,
    ) -> Self {
        Self {
            replay_serializer,
            snapshot_serializer,
            config_artifact_serializer,
            scenario_serializer,
        }
    }

    pub fn build_fixture(
        &self,
        config_artifact: ConfigArtifact<Config>,
        scenario_artifact: Option<SimulationScenario<C, Config>>,
        replay_artifact: ReplayArtifact<C, Snapshot>,
        snapshot_artifact: Option<SnapshotArtifact<Snapshot>>,
    ) -> Result<GoldenFixture<C, Snapshot, Config>, EngineError> {
        validate_fixture_contract(
            &config_artifact,
            scenario_artifact.as_ref(),
            &replay_artifact,
            snapshot_artifact.as_ref(),
        )?;
        let summary = self.fixture_summary_from_artifacts(
            &replay_artifact,
            snapshot_artifact.as_ref(),
            scenario_artifact.as_ref(),
        )?;

        Ok(GoldenFixture {
            metadata: GoldenFixtureMetadata {
                artifact_schema_version: GOLDEN_FIXTURE_SCHEMA_VERSION,
            },
            config_artifact,
            scenario_artifact,
            replay_artifact,
            snapshot_artifact,
            summary,
        })
    }

    pub fn generate_fixture<S, E, Build>(
        &self,
        base_seed: Seed,
        config_artifact: ConfigArtifact<Config>,
        frames: &[InputFrame<C>],
        build: Build,
    ) -> Result<GoldenFixture<C, Snapshot, Config>, EngineError>
    where
        S: SimulationState<Snapshot = Snapshot>,
        E: ReplayableEngine<C, State = S>,
        Build: Fn(Seed, &Config) -> E,
    {
        let config = config_artifact.payload.clone();
        let recorded = record_replay::<C, S, E, _, _, _>(
            base_seed,
            config.snapshot_policy(),
            config_artifact.metadata.identity,
            frames,
            move |seed, _snapshot_policy| build(seed, &config),
            &self.replay_serializer,
        )?;

        self.build_fixture(
            config_artifact,
            None,
            recorded.artifact,
            recorded.result.final_snapshot,
        )
    }

    pub fn generate_fixture_from_scenario<S, E, Build>(
        &self,
        scenario: &SimulationScenario<C, Config>,
        build: Build,
    ) -> Result<GoldenFixture<C, Snapshot, Config>, EngineError>
    where
        S: SimulationState<Snapshot = Snapshot>,
        E: ReplayableEngine<C, State = S>,
        Build: Fn(Seed, &Config) -> E,
    {
        let execution = self
            .scenario_serializer
            .execute::<S, E, Build, SnapshotSerializer>(scenario, build, &self.replay_serializer)?;

        self.build_fixture(
            scenario.config_artifact.clone(),
            Some(scenario.clone()),
            execution.replay.artifact,
            execution.final_snapshot,
        )
    }

    pub fn digest(&self, fixture: &GoldenFixture<C, Snapshot, Config>) -> Result<u64, EngineError> {
        self.encode(fixture).map(|bytes| canonical_digest(&bytes))
    }

    pub fn encode(
        &self,
        fixture: &GoldenFixture<C, Snapshot, Config>,
    ) -> Result<Vec<u8>, EngineError> {
        if fixture.metadata.artifact_schema_version != GOLDEN_FIXTURE_SCHEMA_VERSION {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "golden fixture",
                expected: GOLDEN_FIXTURE_SCHEMA_VERSION,
                got: fixture.metadata.artifact_schema_version,
            });
        }

        validate_fixture_contract(
            &fixture.config_artifact,
            fixture.scenario_artifact.as_ref(),
            &fixture.replay_artifact,
            fixture.snapshot_artifact.as_ref(),
        )?;
        if let Some(detail) = compare_fixture_scenario_contract(fixture, &fixture.config_artifact)?
        {
            return Err(EngineError::ScenarioMismatch { detail });
        }
        let expected_summary = self.fixture_summary_from_artifacts(
            &fixture.replay_artifact,
            fixture.snapshot_artifact.as_ref(),
            fixture.scenario_artifact.as_ref(),
        )?;
        if let Some(detail) = compare_fixture_summary_contract(&fixture.summary, &expected_summary)
        {
            return Err(EngineError::SummaryMismatch { detail });
        }

        let config_bytes = self
            .config_artifact_serializer
            .encode(&fixture.config_artifact)?;
        let scenario_bytes = fixture
            .scenario_artifact
            .as_ref()
            .map(|artifact| self.scenario_serializer.encode(artifact))
            .transpose()?;
        let replay_bytes = self.replay_serializer.encode(&fixture.replay_artifact)?;
        let snapshot_bytes = fixture
            .snapshot_artifact
            .as_ref()
            .map(|artifact| self.snapshot_serializer.encode(artifact))
            .transpose()?;

        let mut writer = CanonicalLineWriter::default();
        writer.push_display("artifact", "golden_fixture");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display(
            "artifact_schema_version",
            fixture.metadata.artifact_schema_version,
        );
        encode_fixture_summary(&mut writer, &fixture.summary);
        writer.push_display("config_artifact_hex", encode_hex(&config_bytes));
        writer.push_display(
            "scenario_artifact.present",
            encode_bool(scenario_bytes.is_some()),
        );
        if let Some(bytes) = scenario_bytes {
            writer.push_display("scenario_artifact_hex", encode_hex(&bytes));
        }
        writer.push_display("replay_artifact_hex", encode_hex(&replay_bytes));
        writer.push_display(
            "snapshot_artifact.present",
            encode_bool(snapshot_bytes.is_some()),
        );
        if let Some(bytes) = snapshot_bytes {
            writer.push_display("snapshot_artifact_hex", encode_hex(&bytes));
        }

        Ok(writer.finish())
    }

    pub fn decode(&self, bytes: &[u8]) -> Result<GoldenFixture<C, Snapshot, Config>, EngineError> {
        let mut reader =
            CanonicalLineReader::new(bytes, "golden fixture").map_err(golden_fixture_corrupted)?;
        expect_fixture_value(&mut reader, "artifact", "golden_fixture")?;
        expect_fixture_value(&mut reader, "canonical_encoding", CANONICAL_TEXT_ENCODING)?;

        let artifact_schema_version =
            parse_u32(read_fixture_value(&mut reader, "artifact_schema_version")?)?;
        if artifact_schema_version != GOLDEN_FIXTURE_SCHEMA_VERSION {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "golden fixture",
                expected: GOLDEN_FIXTURE_SCHEMA_VERSION,
                got: artifact_schema_version,
            });
        }

        let summary = decode_fixture_summary(&mut reader)?;
        let config_artifact_hex = read_fixture_value(&mut reader, "config_artifact_hex")?;
        let config_artifact_bytes =
            decode_hex(config_artifact_hex, "golden fixture config artifact")
                .map_err(golden_fixture_corrupted)?;
        let config_artifact = self
            .config_artifact_serializer
            .decode(&config_artifact_bytes)?;

        let scenario_artifact = if parse_bool(read_fixture_value(
            &mut reader,
            "scenario_artifact.present",
        )?)? {
            let scenario_artifact_hex = read_fixture_value(&mut reader, "scenario_artifact_hex")?;
            let scenario_artifact_bytes =
                decode_hex(scenario_artifact_hex, "golden fixture scenario artifact")
                    .map_err(golden_fixture_corrupted)?;
            Some(self.scenario_serializer.decode(&scenario_artifact_bytes)?)
        } else {
            None
        };

        let replay_artifact_hex = read_fixture_value(&mut reader, "replay_artifact_hex")?;
        let replay_artifact_bytes = decode_hex(replay_artifact_hex, "golden fixture replay")
            .map_err(golden_fixture_corrupted)?;
        let replay_artifact = self.replay_serializer.decode(&replay_artifact_bytes)?;

        let snapshot_artifact = if parse_bool(read_fixture_value(
            &mut reader,
            "snapshot_artifact.present",
        )?)? {
            let snapshot_artifact_hex = read_fixture_value(&mut reader, "snapshot_artifact_hex")?;
            let snapshot_artifact_bytes =
                decode_hex(snapshot_artifact_hex, "golden fixture snapshot")
                    .map_err(golden_fixture_corrupted)?;
            Some(self.snapshot_serializer.decode(&snapshot_artifact_bytes)?)
        } else {
            None
        };

        reader
            .finish("golden fixture")
            .map_err(golden_fixture_corrupted)?;

        let fixture = GoldenFixture {
            metadata: GoldenFixtureMetadata {
                artifact_schema_version,
            },
            config_artifact,
            scenario_artifact,
            replay_artifact,
            snapshot_artifact,
            summary,
        };

        validate_fixture_contract(
            &fixture.config_artifact,
            fixture.scenario_artifact.as_ref(),
            &fixture.replay_artifact,
            fixture.snapshot_artifact.as_ref(),
        )?;
        if let Some(detail) = compare_fixture_scenario_contract(&fixture, &fixture.config_artifact)?
        {
            return Err(EngineError::ScenarioMismatch { detail });
        }
        let expected_summary = self.fixture_summary_from_artifacts(
            &fixture.replay_artifact,
            fixture.snapshot_artifact.as_ref(),
            fixture.scenario_artifact.as_ref(),
        )?;
        if let Some(detail) = compare_fixture_summary_contract(&fixture.summary, &expected_summary)
        {
            return Err(EngineError::SummaryMismatch { detail });
        }

        Ok(fixture)
    }

    pub fn verify_fixture<S, E, Build>(
        &self,
        fixture: &GoldenFixture<C, Snapshot, Config>,
        build: Build,
    ) -> Result<GoldenFixtureResult, EngineError>
    where
        S: SimulationState<Snapshot = Snapshot>,
        E: ReplayableEngine<C, State = S>,
        Build: Fn(Seed, &Config) -> E,
    {
        let (effective_config, base_seed, frames, scenario_digest) =
            match &fixture.scenario_artifact {
                Some(scenario) => (
                    scenario.config_artifact.clone(),
                    scenario.base_seed,
                    scenario.frames.clone(),
                    Some(self.scenario_serializer.digest(scenario)?),
                ),
                None => (
                    fixture.config_artifact.clone(),
                    fixture.replay_artifact.metadata.base_seed,
                    fixture
                        .replay_artifact
                        .records
                        .iter()
                        .map(|record| record.input.clone())
                        .collect(),
                    None,
                ),
            };

        let config_mismatch =
            compare_fixture_config_contract(fixture, &effective_config, scenario_digest)?;
        let scenario_mismatch = compare_fixture_scenario_contract(fixture, &effective_config)?;

        let config = effective_config.payload.clone();
        let recorded = record_replay::<C, S, E, _, _, _>(
            base_seed,
            config.snapshot_policy(),
            effective_config.metadata.identity,
            frames.as_slice(),
            move |seed, _snapshot_policy| build(seed, &config),
            &self.replay_serializer,
        )?;

        let mut actual_summary = ParityArtifactSummary::from(&recorded.result.summary);
        actual_summary.scenario_digest = scenario_digest;
        let comparison = compare_parity_summaries(
            &ParityArtifactSummary::from(&fixture.summary),
            &actual_summary,
        );
        let summary_mismatch = comparison
            .first_mismatch()
            .map(|mismatch| mismatch.to_string());
        let replay_mismatch = compare_replay_traces_with_snapshot_digest(
            fixture.replay_artifact.records.as_slice(),
            recorded.artifact.records.as_slice(),
            |snapshot| self.snapshot_serializer.payload_digest(snapshot),
        )
        .err()
        .map(|mismatch| mismatch.to_string());
        let snapshot_mismatch = compare_optional_snapshot_artifacts(
            fixture.snapshot_artifact.as_ref(),
            recorded.result.final_snapshot.as_ref(),
            &self.snapshot_serializer,
        )?;

        Ok(GoldenFixtureResult {
            fixture_summary: fixture.summary.clone(),
            actual_summary,
            comparison,
            config_mismatch,
            scenario_mismatch,
            replay_mismatch,
            snapshot_mismatch,
            summary_mismatch,
        })
    }

    fn fixture_summary_from_artifacts(
        &self,
        replay_artifact: &ReplayArtifact<C, Snapshot>,
        snapshot_artifact: Option<&SnapshotArtifact<Snapshot>>,
        scenario_artifact: Option<&SimulationScenario<C, Config>>,
    ) -> Result<GoldenFixtureSummary, EngineError> {
        let mut summary =
            GoldenFixtureSummary::from(&self.replay_serializer.summary(replay_artifact)?);
        if let Some(snapshot_artifact) = snapshot_artifact {
            summary.snapshot_digest = Some(self.snapshot_serializer.digest(snapshot_artifact)?);
        }
        if let Some(scenario_artifact) = scenario_artifact {
            summary.scenario_digest = Some(self.scenario_serializer.digest(scenario_artifact)?);
        }
        Ok(summary)
    }
}

fn validate_fixture_contract<C, Snapshot, Config>(
    config_artifact: &ConfigArtifact<Config>,
    scenario_artifact: Option<&SimulationScenario<C, Config>>,
    replay_artifact: &ReplayArtifact<C, Snapshot>,
    snapshot_artifact: Option<&SnapshotArtifact<Snapshot>>,
) -> Result<(), EngineError>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
    Config: Clone + Eq + fmt::Debug + SimulationConfig,
{
    if replay_artifact.metadata.config_identity != config_artifact.metadata.identity {
        return Err(EngineError::ConfigMismatch {
            detail: format!(
                "fixture replay config identity mismatch: expected {:?}, got {:?}",
                config_artifact.metadata.identity, replay_artifact.metadata.config_identity
            ),
        });
    }

    if replay_artifact.metadata.snapshot_policy != config_artifact.payload.snapshot_policy() {
        return Err(EngineError::ConfigMismatch {
            detail: format!(
                "fixture replay snapshot policy mismatch: expected {:?}, got {:?}",
                config_artifact.payload.snapshot_policy(),
                replay_artifact.metadata.snapshot_policy
            ),
        });
    }

    if let Some(snapshot_artifact) = snapshot_artifact {
        if snapshot_artifact.metadata.base_seed != replay_artifact.metadata.base_seed {
            return Err(EngineError::SnapshotMetadataMismatch {
                detail: format!(
                    "fixture snapshot base seed mismatch: expected {}, got {}",
                    replay_artifact.metadata.base_seed, snapshot_artifact.metadata.base_seed
                ),
            });
        }

        if snapshot_artifact.metadata.config_identity != config_artifact.metadata.identity {
            return Err(EngineError::ConfigMismatch {
                detail: format!(
                    "fixture snapshot config identity mismatch: expected {:?}, got {:?}",
                    config_artifact.metadata.identity, snapshot_artifact.metadata.config_identity
                ),
            });
        }
    }

    if let Some(scenario_artifact) = scenario_artifact {
        if scenario_artifact.config_artifact != *config_artifact {
            return Err(EngineError::ConfigMismatch {
                detail: format!(
                    "fixture config artifact does not match scenario config artifact: expected {:?}, got {:?}",
                    scenario_artifact.config_artifact, config_artifact
                ),
            });
        }
    }

    Ok(())
}

fn compare_fixture_config_contract<C, Snapshot, Config>(
    fixture: &GoldenFixture<C, Snapshot, Config>,
    effective_config: &ConfigArtifact<Config>,
    scenario_digest: Option<u64>,
) -> Result<Option<String>, EngineError>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
    Config: Clone + Eq + fmt::Debug + SimulationConfig,
{
    if fixture.config_artifact.metadata.identity != effective_config.metadata.identity {
        return Ok(Some(format!(
            "fixture config identity mismatch: fixture {:?}, effective {:?}",
            fixture.config_artifact.metadata.identity, effective_config.metadata.identity
        )));
    }

    if fixture.replay_artifact.metadata.config_identity != effective_config.metadata.identity {
        return Ok(Some(format!(
            "replay config identity mismatch: expected {:?}, got {:?}",
            effective_config.metadata.identity, fixture.replay_artifact.metadata.config_identity
        )));
    }

    if fixture.replay_artifact.metadata.snapshot_policy
        != effective_config.payload.snapshot_policy()
    {
        return Ok(Some(format!(
            "replay snapshot policy mismatch: expected {:?}, got {:?}",
            effective_config.payload.snapshot_policy(),
            fixture.replay_artifact.metadata.snapshot_policy
        )));
    }

    if fixture.summary.config_payload_schema_version
        != effective_config.metadata.identity.payload_schema_version
    {
        return Ok(Some(format!(
            "fixture summary config schema mismatch: expected {}, got {}",
            effective_config.metadata.identity.payload_schema_version,
            fixture.summary.config_payload_schema_version
        )));
    }

    if fixture.summary.config_digest != effective_config.metadata.identity.digest {
        return Ok(Some(format!(
            "fixture summary config digest mismatch: expected {}, got {}",
            effective_config.metadata.identity.digest, fixture.summary.config_digest
        )));
    }

    if fixture.summary.scenario_digest != scenario_digest {
        return Ok(Some(format!(
            "fixture summary scenario digest mismatch: expected {:?}, got {:?}",
            scenario_digest, fixture.summary.scenario_digest
        )));
    }

    if let Some(snapshot_artifact) = &fixture.snapshot_artifact
        && snapshot_artifact.metadata.config_identity != effective_config.metadata.identity
    {
        return Ok(Some(format!(
            "snapshot config identity mismatch: expected {:?}, got {:?}",
            effective_config.metadata.identity, snapshot_artifact.metadata.config_identity
        )));
    }

    Ok(None)
}

fn compare_fixture_scenario_contract<C, Snapshot, Config>(
    fixture: &GoldenFixture<C, Snapshot, Config>,
    effective_config: &ConfigArtifact<Config>,
) -> Result<Option<String>, EngineError>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
    Config: Clone + Eq + fmt::Debug + SimulationConfig,
{
    let Some(scenario) = fixture.scenario_artifact.as_ref() else {
        return Ok(None);
    };

    if scenario.config_artifact != *effective_config {
        return Ok(Some(format!(
            "scenario config artifact mismatch: expected {:?}, got {:?}",
            effective_config, scenario.config_artifact
        )));
    }

    if scenario.base_seed != fixture.replay_artifact.metadata.base_seed {
        return Ok(Some(format!(
            "scenario base seed mismatch: expected {}, got {}",
            fixture.replay_artifact.metadata.base_seed, scenario.base_seed
        )));
    }

    let replay_frames = fixture
        .replay_artifact
        .records
        .iter()
        .map(|record| record.input.clone())
        .collect::<Vec<_>>();
    if scenario.frames != replay_frames {
        return Ok(Some(format!(
            "scenario frames mismatch: expected {:?}, got {:?}",
            replay_frames, scenario.frames
        )));
    }

    Ok(None)
}

fn compare_fixture_summary_contract(
    actual: &GoldenFixtureSummary,
    expected: &GoldenFixtureSummary,
) -> Option<String> {
    let comparison = compare_parity_summaries(
        &ParityArtifactSummary::from(expected),
        &ParityArtifactSummary::from(actual),
    );
    if comparison.is_match() {
        None
    } else {
        Some(comparison.to_string())
    }
}

fn compare_optional_snapshot_artifacts<Snapshot, SnapshotSerializer>(
    expected: Option<&SnapshotArtifact<Snapshot>>,
    actual: Option<&SnapshotArtifact<Snapshot>>,
    snapshot_serializer: &SnapshotArtifactSerializer<Snapshot, SnapshotSerializer>,
) -> Result<Option<String>, EngineError>
where
    Snapshot: Clone + Eq + fmt::Debug,
    SnapshotSerializer: Serializer<Snapshot>,
    SnapshotSerializer::Error: fmt::Display,
{
    match (expected, actual) {
        (None, None) => Ok(None),
        (Some(expected), None) => Ok(Some(format!(
            "snapshot artifact missing from rerun: expected tick {}",
            expected.metadata.snapshot.source_tick
        ))),
        (None, Some(actual)) => Ok(Some(format!(
            "unexpected rerun snapshot artifact at tick {}",
            actual.metadata.snapshot.source_tick
        ))),
        (Some(expected), Some(actual)) => {
            compare_snapshot_artifacts(expected, actual, snapshot_serializer)
        }
    }
}

fn compare_snapshot_artifacts<Snapshot, SnapshotSerializer>(
    expected: &SnapshotArtifact<Snapshot>,
    actual: &SnapshotArtifact<Snapshot>,
    snapshot_serializer: &SnapshotArtifactSerializer<Snapshot, SnapshotSerializer>,
) -> Result<Option<String>, EngineError>
where
    Snapshot: Clone + Eq + fmt::Debug,
    SnapshotSerializer: Serializer<Snapshot>,
    SnapshotSerializer::Error: fmt::Display,
{
    if expected.metadata.base_seed != actual.metadata.base_seed {
        return Ok(Some(format!(
            "snapshot base seed mismatch: expected {}, got {}",
            expected.metadata.base_seed, actual.metadata.base_seed
        )));
    }

    if expected.metadata.config_identity != actual.metadata.config_identity {
        return Ok(Some(format!(
            "snapshot config identity mismatch: expected {:?}, got {:?}",
            expected.metadata.config_identity, actual.metadata.config_identity
        )));
    }

    if expected.metadata.engine_family != actual.metadata.engine_family {
        return Ok(Some(format!(
            "snapshot engine family mismatch: expected `{}`, got `{}`",
            expected.metadata.engine_family, actual.metadata.engine_family
        )));
    }

    if expected.metadata.capture_reason != actual.metadata.capture_reason {
        return Ok(Some(format!(
            "snapshot capture reason mismatch: expected {:?}, got {:?}",
            expected.metadata.capture_reason, actual.metadata.capture_reason
        )));
    }

    if expected.metadata.snapshot != actual.metadata.snapshot {
        return Ok(Some(format!(
            "snapshot metadata mismatch: expected {:?}, got {:?}",
            expected.metadata.snapshot, actual.metadata.snapshot
        )));
    }

    if expected.payload != actual.payload {
        let expected_digest = snapshot_serializer.payload_digest(&expected.payload)?;
        let actual_digest = snapshot_serializer.payload_digest(&actual.payload)?;
        return Ok(Some(format!(
            "snapshot payload digest mismatch: expected {}, got {}",
            expected_digest, actual_digest
        )));
    }

    Ok(None)
}

fn encode_fixture_summary(writer: &mut CanonicalLineWriter, summary: &GoldenFixtureSummary) {
    writer.push_str_hex("summary.engine_family_hex", &summary.engine_family);
    writer.push_display("summary.base_seed", summary.base_seed);
    writer.push_display("summary.final_tick", summary.final_tick);
    writer.push_display("summary.final_checksum", summary.final_checksum);
    writer.push_display(
        "summary.config_payload_schema_version",
        summary.config_payload_schema_version,
    );
    writer.push_display("summary.config_digest", summary.config_digest);
    writer.push_display(
        "summary.replay_artifact_schema_version",
        summary.replay_artifact_schema_version,
    );
    writer.push_display(
        "summary.snapshot_artifact_schema_version",
        summary.snapshot_artifact_schema_version,
    );
    writer.push_display(
        "summary.command_payload_schema_version",
        summary.command_payload_schema_version,
    );
    writer.push_display(
        "summary.snapshot_payload_schema_version",
        summary.snapshot_payload_schema_version,
    );
    writer.push_display("summary.replay_digest", summary.replay_digest);
    writer.push_display(
        "summary.snapshot_digest.present",
        encode_bool(summary.snapshot_digest.is_some()),
    );
    if let Some(snapshot_digest) = summary.snapshot_digest {
        writer.push_display("summary.snapshot_digest", snapshot_digest);
    }
    writer.push_display(
        "summary.scenario_digest.present",
        encode_bool(summary.scenario_digest.is_some()),
    );
    if let Some(scenario_digest) = summary.scenario_digest {
        writer.push_display("summary.scenario_digest", scenario_digest);
    }
}

fn decode_fixture_summary(
    reader: &mut CanonicalLineReader<'_>,
) -> Result<GoldenFixtureSummary, EngineError> {
    Ok(GoldenFixtureSummary {
        engine_family: decode_hex_string(
            read_fixture_value(reader, "summary.engine_family_hex")?,
            "golden fixture summary engine family",
        )
        .map_err(golden_fixture_corrupted)?,
        base_seed: parse_u64(read_fixture_value(reader, "summary.base_seed")?)?,
        final_tick: parse_u64(read_fixture_value(reader, "summary.final_tick")?)?,
        final_checksum: parse_u64(read_fixture_value(reader, "summary.final_checksum")?)?,
        config_payload_schema_version: parse_u32(read_fixture_value(
            reader,
            "summary.config_payload_schema_version",
        )?)?,
        config_digest: parse_u64(read_fixture_value(reader, "summary.config_digest")?)?,
        replay_artifact_schema_version: parse_u32(read_fixture_value(
            reader,
            "summary.replay_artifact_schema_version",
        )?)?,
        snapshot_artifact_schema_version: parse_u32(read_fixture_value(
            reader,
            "summary.snapshot_artifact_schema_version",
        )?)?,
        command_payload_schema_version: parse_u32(read_fixture_value(
            reader,
            "summary.command_payload_schema_version",
        )?)?,
        snapshot_payload_schema_version: parse_u32(read_fixture_value(
            reader,
            "summary.snapshot_payload_schema_version",
        )?)?,
        replay_digest: parse_u64(read_fixture_value(reader, "summary.replay_digest")?)?,
        snapshot_digest: {
            let present = parse_bool(read_fixture_value(
                reader,
                "summary.snapshot_digest.present",
            )?)?;
            if present {
                Some(parse_u64(read_fixture_value(
                    reader,
                    "summary.snapshot_digest",
                )?)?)
            } else {
                None
            }
        },
        scenario_digest: {
            let present = parse_bool(read_fixture_value(
                reader,
                "summary.scenario_digest.present",
            )?)?;
            if present {
                Some(parse_u64(read_fixture_value(
                    reader,
                    "summary.scenario_digest",
                )?)?)
            } else {
                None
            }
        },
    })
}

fn golden_fixture_corrupted(error: impl ToString) -> EngineError {
    EngineError::CorruptedArtifact {
        artifact: "golden fixture",
        detail: error.to_string(),
    }
}

fn read_fixture_value<'a>(
    reader: &mut CanonicalLineReader<'a>,
    key: &str,
) -> Result<&'a str, EngineError> {
    reader
        .read_value(key, "golden fixture")
        .map_err(golden_fixture_corrupted)
}

fn expect_fixture_value(
    reader: &mut CanonicalLineReader<'_>,
    key: &str,
    expected: &str,
) -> Result<(), EngineError> {
    reader
        .expect_value(key, expected, "golden fixture")
        .map_err(golden_fixture_corrupted)
}

fn encode_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn parse_bool(value: &str) -> Result<bool, EngineError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(EngineError::CorruptedArtifact {
            artifact: "golden fixture",
            detail: format!("invalid bool `{value}`"),
        }),
    }
}

fn parse_u32(value: &str) -> Result<u32, EngineError> {
    value
        .parse()
        .map_err(|error| EngineError::CorruptedArtifact {
            artifact: "golden fixture",
            detail: format!("invalid u32 `{value}`: {error}"),
        })
}

fn parse_u64(value: &str) -> Result<u64, EngineError> {
    value
        .parse()
        .map_err(|error| EngineError::CorruptedArtifact {
            artifact: "golden fixture",
            detail: format!("invalid u64 `{value}`: {error}"),
        })
}
