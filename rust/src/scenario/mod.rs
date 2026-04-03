use std::fmt;
use std::marker::PhantomData;

use crate::canonical::{
    CANONICAL_TEXT_ENCODING, CanonicalLineReader, CanonicalLineWriter, canonical_digest,
    decode_hex, decode_hex_string, encode_hex,
};
use crate::config::{ConfigArtifact, ConfigArtifactSerializer, SimulationConfig};
use crate::core::{EngineError, Seed};
use crate::engine::ReplayableEngine;
use crate::input::{Command, InputFrame};
use crate::parity::{ParityArtifactSummary, ParityComparison, compare_parity_summaries};
use crate::persistence::{
    RecordedReplay, ReplayArtifactSerializer, SnapshotArtifact, record_replay,
};
use crate::replay::{ReplayInspectionView, inspect_replay_trace};
use crate::serialization::Serializer;
use crate::state::SimulationState;

pub const SCENARIO_ARTIFACT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationScenarioMetadata {
    pub artifact_schema_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationScenario<C, Config>
where
    C: Command,
    Config: Clone + Eq + fmt::Debug,
{
    pub metadata: SimulationScenarioMetadata,
    pub config_artifact: ConfigArtifact<Config>,
    pub base_seed: Seed,
    pub frames: Vec<InputFrame<C>>,
    pub expected_parity_summary: Option<ParityArtifactSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScenarioExecutionResult<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub scenario_digest: u64,
    pub replay: RecordedReplay<C, Snapshot>,
    pub parity_summary: ParityArtifactSummary,
    pub final_snapshot: Option<SnapshotArtifact<Snapshot>>,
    pub inspection: ReplayInspectionView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScenarioVerificationResult<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub execution: ScenarioExecutionResult<C, Snapshot>,
    pub parity_comparison: Option<ParityComparison>,
}

impl<C, Snapshot> ScenarioVerificationResult<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub fn passed(&self) -> bool {
        self.parity_comparison
            .as_ref()
            .map(|comparison| comparison.is_match())
            .unwrap_or(true)
    }
}

#[derive(Clone, Debug)]
pub struct SimulationScenarioSerializer<C, Config, CommandSerializer, ConfigSerializer>
where
    C: Command,
    Config: Clone + Eq + fmt::Debug,
{
    command_serializer: CommandSerializer,
    config_artifact_serializer: ConfigArtifactSerializer<Config, ConfigSerializer>,
    _marker: PhantomData<C>,
}

impl<C, Config, CommandSerializer, ConfigSerializer>
    SimulationScenarioSerializer<C, Config, CommandSerializer, ConfigSerializer>
where
    C: Command,
    Config: Clone + Eq + fmt::Debug,
    CommandSerializer: Serializer<C>,
    ConfigSerializer: Serializer<Config>,
    CommandSerializer::Error: fmt::Display,
    ConfigSerializer::Error: fmt::Display,
{
    pub fn new(
        command_serializer: CommandSerializer,
        config_artifact_serializer: ConfigArtifactSerializer<Config, ConfigSerializer>,
    ) -> Self {
        Self {
            command_serializer,
            config_artifact_serializer,
            _marker: PhantomData,
        }
    }

    pub fn build_scenario(
        &self,
        config_artifact: ConfigArtifact<Config>,
        base_seed: Seed,
        frames: &[InputFrame<C>],
        expected_parity_summary: Option<ParityArtifactSummary>,
    ) -> SimulationScenario<C, Config> {
        SimulationScenario {
            metadata: SimulationScenarioMetadata {
                artifact_schema_version: SCENARIO_ARTIFACT_SCHEMA_VERSION,
            },
            config_artifact,
            base_seed,
            frames: frames.to_vec(),
            expected_parity_summary,
        }
    }

    pub fn digest(&self, scenario: &SimulationScenario<C, Config>) -> Result<u64, EngineError> {
        self.contract_bytes(scenario)
            .map(|bytes| canonical_digest(&bytes))
    }

    pub fn execute<S, E, Build, SnapshotSerializer>(
        &self,
        scenario: &SimulationScenario<C, Config>,
        build: Build,
        replay_serializer: &ReplayArtifactSerializer<
            C,
            S::Snapshot,
            CommandSerializer,
            SnapshotSerializer,
        >,
    ) -> Result<ScenarioExecutionResult<C, S::Snapshot>, EngineError>
    where
        S: SimulationState,
        E: ReplayableEngine<C, State = S>,
        Build: Fn(Seed, &Config) -> E,
        Config: SimulationConfig,
        SnapshotSerializer: Serializer<S::Snapshot>,
        SnapshotSerializer::Error: fmt::Display,
    {
        let scenario_digest = self.digest(scenario)?;
        let config = scenario.config_artifact.payload.clone();
        let config_identity = scenario.config_artifact.metadata.identity;
        let recorded = record_replay::<C, S, E, _, _, _>(
            scenario.base_seed,
            config.snapshot_policy(),
            config_identity,
            scenario.frames.as_slice(),
            move |seed, _snapshot_policy| build(seed, &config),
            replay_serializer,
        )?;

        let mut recorded = recorded;
        recorded.result.summary.scenario_digest = Some(scenario_digest);
        let parity_summary = ParityArtifactSummary::from(&recorded.result.summary);
        let inspection = inspect_replay_trace(recorded.artifact.records.as_slice());
        let final_snapshot = recorded.result.final_snapshot.clone();

        Ok(ScenarioExecutionResult {
            scenario_digest,
            replay: recorded,
            parity_summary,
            final_snapshot,
            inspection,
        })
    }

    pub fn verify<S, E, Build, SnapshotSerializer>(
        &self,
        scenario: &SimulationScenario<C, Config>,
        build: Build,
        replay_serializer: &ReplayArtifactSerializer<
            C,
            S::Snapshot,
            CommandSerializer,
            SnapshotSerializer,
        >,
    ) -> Result<ScenarioVerificationResult<C, S::Snapshot>, EngineError>
    where
        S: SimulationState,
        E: ReplayableEngine<C, State = S>,
        Build: Fn(Seed, &Config) -> E,
        Config: SimulationConfig,
        SnapshotSerializer: Serializer<S::Snapshot>,
        SnapshotSerializer::Error: fmt::Display,
    {
        let execution =
            self.execute::<S, E, Build, SnapshotSerializer>(scenario, build, replay_serializer)?;
        let parity_comparison = scenario
            .expected_parity_summary
            .as_ref()
            .map(|expected| compare_parity_summaries(expected, &execution.parity_summary));

        Ok(ScenarioVerificationResult {
            execution,
            parity_comparison,
        })
    }

    fn contract_bytes(
        &self,
        scenario: &SimulationScenario<C, Config>,
    ) -> Result<Vec<u8>, EngineError> {
        let config_bytes = self
            .config_artifact_serializer
            .encode(&scenario.config_artifact)?;
        let mut writer = CanonicalLineWriter::default();
        writer.push_display("artifact", "scenario_contract");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display(
            "artifact_schema_version",
            scenario.metadata.artifact_schema_version,
        );
        writer.push_display("config_artifact_hex", encode_hex(&config_bytes));
        writer.push_display("base_seed", scenario.base_seed);
        writer.push_display("frame_count", scenario.frames.len());

        for (index, frame) in scenario.frames.iter().enumerate() {
            let command_bytes =
                self.command_serializer
                    .encode(&frame.command)
                    .map_err(|error| EngineError::ScenarioDecode {
                        detail: format!(
                            "failed to encode scenario command at frame {index}: {error}"
                        ),
                    })?;
            writer.push_display(format!("frame.{index}.tick").as_str(), frame.tick);
            writer.push_display(
                format!("frame.{index}.command_hex").as_str(),
                encode_hex(&command_bytes),
            );
        }

        Ok(writer.finish())
    }
}

impl<C, Config, CommandSerializer, ConfigSerializer> Serializer<SimulationScenario<C, Config>>
    for SimulationScenarioSerializer<C, Config, CommandSerializer, ConfigSerializer>
where
    C: Command,
    Config: Clone + Eq + fmt::Debug,
    CommandSerializer: Serializer<C>,
    ConfigSerializer: Serializer<Config>,
    CommandSerializer::Error: fmt::Display,
    ConfigSerializer::Error: fmt::Display,
{
    type Error = EngineError;

    fn schema_version(&self) -> u32 {
        SCENARIO_ARTIFACT_SCHEMA_VERSION
    }

    fn encode(&self, value: &SimulationScenario<C, Config>) -> Result<Vec<u8>, Self::Error> {
        if value.metadata.artifact_schema_version != self.schema_version() {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "scenario artifact",
                expected: self.schema_version(),
                got: value.metadata.artifact_schema_version,
            });
        }

        let config_bytes = self
            .config_artifact_serializer
            .encode(&value.config_artifact)?;
        let mut writer = CanonicalLineWriter::default();
        writer.push_display("artifact", "scenario");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display(
            "artifact_schema_version",
            value.metadata.artifact_schema_version,
        );
        writer.push_display("config_artifact_hex", encode_hex(&config_bytes));
        writer.push_display("base_seed", value.base_seed);
        writer.push_display("frame_count", value.frames.len());

        for (index, frame) in value.frames.iter().enumerate() {
            let command_bytes =
                self.command_serializer
                    .encode(&frame.command)
                    .map_err(|error| EngineError::ScenarioDecode {
                        detail: format!(
                            "failed to encode scenario command at frame {index}: {error}"
                        ),
                    })?;
            writer.push_display(format!("frame.{index}.tick").as_str(), frame.tick);
            writer.push_display(
                format!("frame.{index}.command_hex").as_str(),
                encode_hex(&command_bytes),
            );
        }

        writer.push_display(
            "expected_parity.present",
            encode_bool(value.expected_parity_summary.is_some()),
        );
        if let Some(summary) = &value.expected_parity_summary {
            encode_parity_summary(&mut writer, "expected_parity", summary);
        }

        Ok(writer.finish())
    }

    fn decode(&self, bytes: &[u8]) -> Result<SimulationScenario<C, Config>, Self::Error> {
        let mut reader = CanonicalLineReader::new(bytes, "scenario artifact").map_err(|error| {
            EngineError::CorruptedArtifact {
                artifact: "scenario",
                detail: error.to_string(),
            }
        })?;
        expect_scenario_value(&mut reader, "artifact", "scenario")?;
        expect_scenario_value(&mut reader, "canonical_encoding", CANONICAL_TEXT_ENCODING)?;

        let artifact_schema_version =
            parse_u32(read_scenario_value(&mut reader, "artifact_schema_version")?)?;
        if artifact_schema_version != self.schema_version() {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "scenario artifact",
                expected: self.schema_version(),
                got: artifact_schema_version,
            });
        }

        let config_artifact_hex = read_scenario_value(&mut reader, "config_artifact_hex")?;
        let config_artifact_bytes = decode_hex(config_artifact_hex, "scenario config artifact")
            .map_err(scenario_corrupted)?;
        let config_artifact = self
            .config_artifact_serializer
            .decode(&config_artifact_bytes)?;

        let base_seed = parse_u64(read_scenario_value(&mut reader, "base_seed")?)?;
        let frame_count = parse_usize(read_scenario_value(&mut reader, "frame_count")?)?;
        let mut frames = Vec::with_capacity(frame_count);

        for index in 0..frame_count {
            let tick = parse_u64(read_scenario_value(
                &mut reader,
                format!("frame.{index}.tick").as_str(),
            )?)?;
            let command_hex =
                read_scenario_value(&mut reader, format!("frame.{index}.command_hex").as_str())?;
            let command_bytes =
                decode_hex(command_hex, "scenario command").map_err(scenario_corrupted)?;
            let command = self
                .command_serializer
                .decode(&command_bytes)
                .map_err(|error| EngineError::ScenarioDecode {
                    detail: format!("failed to decode scenario command at frame {index}: {error}"),
                })?;
            frames.push(InputFrame::new(tick, command));
        }

        let expected_parity_summary =
            if parse_bool(read_scenario_value(&mut reader, "expected_parity.present")?)? {
                Some(decode_parity_summary(&mut reader, "expected_parity")?)
            } else {
                None
            };

        reader
            .finish("scenario artifact")
            .map_err(scenario_corrupted)?;

        Ok(SimulationScenario {
            metadata: SimulationScenarioMetadata {
                artifact_schema_version,
            },
            config_artifact,
            base_seed,
            frames,
            expected_parity_summary,
        })
    }
}

fn encode_parity_summary(
    writer: &mut CanonicalLineWriter,
    prefix: &str,
    summary: &ParityArtifactSummary,
) {
    writer.push_str_hex(
        format!("{prefix}.engine_family_hex").as_str(),
        &summary.engine_family,
    );
    writer.push_display(format!("{prefix}.base_seed").as_str(), summary.base_seed);
    writer.push_display(format!("{prefix}.final_tick").as_str(), summary.final_tick);
    writer.push_display(
        format!("{prefix}.final_checksum").as_str(),
        summary.final_checksum,
    );
    writer.push_display(
        format!("{prefix}.config_payload_schema_version").as_str(),
        summary.config_payload_schema_version,
    );
    writer.push_display(
        format!("{prefix}.config_digest").as_str(),
        summary.config_digest,
    );
    writer.push_display(
        format!("{prefix}.replay_artifact_schema_version").as_str(),
        summary.replay_artifact_schema_version,
    );
    writer.push_display(
        format!("{prefix}.snapshot_artifact_schema_version").as_str(),
        summary.snapshot_artifact_schema_version,
    );
    writer.push_display(
        format!("{prefix}.command_payload_schema_version").as_str(),
        summary.command_payload_schema_version,
    );
    writer.push_display(
        format!("{prefix}.snapshot_payload_schema_version").as_str(),
        summary.snapshot_payload_schema_version,
    );
    writer.push_display(
        format!("{prefix}.replay_digest").as_str(),
        summary.replay_digest,
    );
    writer.push_display(
        format!("{prefix}.snapshot_digest.present").as_str(),
        encode_bool(summary.snapshot_digest.is_some()),
    );
    if let Some(snapshot_digest) = summary.snapshot_digest {
        writer.push_display(
            format!("{prefix}.snapshot_digest").as_str(),
            snapshot_digest,
        );
    }
    writer.push_display(
        format!("{prefix}.scenario_digest.present").as_str(),
        encode_bool(summary.scenario_digest.is_some()),
    );
    if let Some(scenario_digest) = summary.scenario_digest {
        writer.push_display(
            format!("{prefix}.scenario_digest").as_str(),
            scenario_digest,
        );
    }
}

fn decode_parity_summary(
    reader: &mut CanonicalLineReader<'_>,
    prefix: &str,
) -> Result<ParityArtifactSummary, EngineError> {
    Ok(ParityArtifactSummary {
        engine_family: decode_hex_string(
            read_scenario_value(reader, format!("{prefix}.engine_family_hex").as_str())?,
            "scenario parity engine family",
        )
        .map_err(scenario_corrupted)?,
        base_seed: parse_u64(read_scenario_value(
            reader,
            format!("{prefix}.base_seed").as_str(),
        )?)?,
        final_tick: parse_u64(read_scenario_value(
            reader,
            format!("{prefix}.final_tick").as_str(),
        )?)?,
        final_checksum: parse_u64(read_scenario_value(
            reader,
            format!("{prefix}.final_checksum").as_str(),
        )?)?,
        config_payload_schema_version: parse_u32(read_scenario_value(
            reader,
            format!("{prefix}.config_payload_schema_version").as_str(),
        )?)?,
        config_digest: parse_u64(read_scenario_value(
            reader,
            format!("{prefix}.config_digest").as_str(),
        )?)?,
        replay_artifact_schema_version: parse_u32(read_scenario_value(
            reader,
            format!("{prefix}.replay_artifact_schema_version").as_str(),
        )?)?,
        snapshot_artifact_schema_version: parse_u32(read_scenario_value(
            reader,
            format!("{prefix}.snapshot_artifact_schema_version").as_str(),
        )?)?,
        command_payload_schema_version: parse_u32(read_scenario_value(
            reader,
            format!("{prefix}.command_payload_schema_version").as_str(),
        )?)?,
        snapshot_payload_schema_version: parse_u32(read_scenario_value(
            reader,
            format!("{prefix}.snapshot_payload_schema_version").as_str(),
        )?)?,
        replay_digest: parse_u64(read_scenario_value(
            reader,
            format!("{prefix}.replay_digest").as_str(),
        )?)?,
        snapshot_digest: {
            let present = parse_bool(read_scenario_value(
                reader,
                format!("{prefix}.snapshot_digest.present").as_str(),
            )?)?;
            if present {
                Some(parse_u64(read_scenario_value(
                    reader,
                    format!("{prefix}.snapshot_digest").as_str(),
                )?)?)
            } else {
                None
            }
        },
        scenario_digest: {
            let present = parse_bool(read_scenario_value(
                reader,
                format!("{prefix}.scenario_digest.present").as_str(),
            )?)?;
            if present {
                Some(parse_u64(read_scenario_value(
                    reader,
                    format!("{prefix}.scenario_digest").as_str(),
                )?)?)
            } else {
                None
            }
        },
    })
}

fn scenario_corrupted(error: impl ToString) -> EngineError {
    EngineError::CorruptedArtifact {
        artifact: "scenario",
        detail: error.to_string(),
    }
}

fn read_scenario_value<'a>(
    reader: &mut CanonicalLineReader<'a>,
    key: &str,
) -> Result<&'a str, EngineError> {
    reader
        .read_value(key, "scenario artifact")
        .map_err(scenario_corrupted)
}

fn expect_scenario_value(
    reader: &mut CanonicalLineReader<'_>,
    key: &str,
    expected: &str,
) -> Result<(), EngineError> {
    reader
        .expect_value(key, expected, "scenario artifact")
        .map_err(scenario_corrupted)
}

fn encode_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn parse_bool(value: &str) -> Result<bool, EngineError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(EngineError::CorruptedArtifact {
            artifact: "scenario",
            detail: format!("invalid bool `{value}`"),
        }),
    }
}

fn parse_u32(value: &str) -> Result<u32, EngineError> {
    value
        .parse()
        .map_err(|error| EngineError::CorruptedArtifact {
            artifact: "scenario",
            detail: format!("invalid u32 `{value}`: {error}"),
        })
}

fn parse_u64(value: &str) -> Result<u64, EngineError> {
    value
        .parse()
        .map_err(|error| EngineError::CorruptedArtifact {
            artifact: "scenario",
            detail: format!("invalid u64 `{value}`: {error}"),
        })
}

fn parse_usize(value: &str) -> Result<usize, EngineError> {
    value
        .parse()
        .map_err(|error| EngineError::CorruptedArtifact {
            artifact: "scenario",
            detail: format!("invalid usize `{value}`: {error}"),
        })
}
