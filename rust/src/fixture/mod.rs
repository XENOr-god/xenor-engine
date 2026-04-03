use std::fmt;

use crate::canonical::{
    CANONICAL_TEXT_ENCODING, CanonicalLineReader, CanonicalLineWriter, decode_hex,
    decode_hex_string, encode_hex,
};
use crate::core::{EngineError, Seed, Tick};
use crate::engine::{ReplayableEngine, SnapshotPolicy};
use crate::input::{Command, InputFrame};
use crate::parity::{ParityArtifactSummary, ParityComparison, compare_parity_summaries};
use crate::persistence::{
    ArtifactSummary, ReplayArtifact, ReplayArtifactSerializer, SnapshotArtifact,
    SnapshotArtifactSerializer, record_replay,
};
use crate::replay::compare_replay_traces_with_snapshot_digest;
use crate::serialization::Serializer;
use crate::state::SimulationState;

pub const GOLDEN_FIXTURE_SCHEMA_VERSION: u32 = 1;

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
    pub replay_artifact_schema_version: u32,
    pub snapshot_artifact_schema_version: u32,
    pub command_payload_schema_version: u32,
    pub snapshot_payload_schema_version: u32,
    pub replay_digest: u64,
    pub snapshot_digest: Option<u64>,
}

impl From<&ArtifactSummary> for GoldenFixtureSummary {
    fn from(value: &ArtifactSummary) -> Self {
        Self {
            engine_family: value.engine_family.clone(),
            base_seed: value.base_seed,
            final_tick: value.final_tick,
            final_checksum: value.final_checksum,
            replay_artifact_schema_version: value.replay_artifact_schema_version,
            snapshot_artifact_schema_version: value.snapshot_artifact_schema_version,
            command_payload_schema_version: value.command_payload_schema_version,
            snapshot_payload_schema_version: value.snapshot_payload_schema_version,
            replay_digest: value.replay_digest,
            snapshot_digest: value.snapshot_digest,
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
            replay_artifact_schema_version: value.replay_artifact_schema_version,
            snapshot_artifact_schema_version: value.snapshot_artifact_schema_version,
            command_payload_schema_version: value.command_payload_schema_version,
            snapshot_payload_schema_version: value.snapshot_payload_schema_version,
            replay_digest: value.replay_digest,
            snapshot_digest: value.snapshot_digest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenFixture<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub metadata: GoldenFixtureMetadata,
    pub replay_artifact: ReplayArtifact<C, Snapshot>,
    pub snapshot_artifact: Option<SnapshotArtifact<Snapshot>>,
    pub summary: GoldenFixtureSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenFixtureResult {
    pub fixture_summary: GoldenFixtureSummary,
    pub actual_summary: ParityArtifactSummary,
    pub comparison: ParityComparison,
    pub replay_mismatch: Option<String>,
    pub snapshot_mismatch: Option<String>,
}

impl GoldenFixtureResult {
    pub fn passed(&self) -> bool {
        self.comparison.is_match()
            && self.replay_mismatch.is_none()
            && self.snapshot_mismatch.is_none()
    }
}

#[derive(Clone, Debug)]
pub struct GoldenFixtureSerializer<C, Snapshot, CommandSerializer, SnapshotSerializer>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    replay_serializer: ReplayArtifactSerializer<C, Snapshot, CommandSerializer, SnapshotSerializer>,
    snapshot_serializer: SnapshotArtifactSerializer<Snapshot, SnapshotSerializer>,
}

impl<C, Snapshot, CommandSerializer, SnapshotSerializer>
    GoldenFixtureSerializer<C, Snapshot, CommandSerializer, SnapshotSerializer>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
    CommandSerializer: Serializer<C>,
    SnapshotSerializer: Serializer<Snapshot>,
    CommandSerializer::Error: fmt::Display,
    SnapshotSerializer::Error: fmt::Display,
{
    pub fn new(
        replay_serializer: ReplayArtifactSerializer<
            C,
            Snapshot,
            CommandSerializer,
            SnapshotSerializer,
        >,
        snapshot_serializer: SnapshotArtifactSerializer<Snapshot, SnapshotSerializer>,
    ) -> Self {
        Self {
            replay_serializer,
            snapshot_serializer,
        }
    }

    pub fn build_fixture(
        &self,
        replay_artifact: ReplayArtifact<C, Snapshot>,
        snapshot_artifact: Option<SnapshotArtifact<Snapshot>>,
    ) -> Result<GoldenFixture<C, Snapshot>, EngineError> {
        let summary =
            self.fixture_summary_from_artifacts(&replay_artifact, snapshot_artifact.as_ref())?;

        Ok(GoldenFixture {
            metadata: GoldenFixtureMetadata {
                artifact_schema_version: GOLDEN_FIXTURE_SCHEMA_VERSION,
            },
            replay_artifact,
            snapshot_artifact,
            summary,
        })
    }

    pub fn generate_fixture<S, E, Build>(
        &self,
        base_seed: Seed,
        snapshot_policy: SnapshotPolicy,
        frames: &[InputFrame<C>],
        build: Build,
    ) -> Result<GoldenFixture<C, Snapshot>, EngineError>
    where
        S: SimulationState<Snapshot = Snapshot>,
        E: ReplayableEngine<C, State = S>,
        Build: Fn(Seed, SnapshotPolicy) -> E,
    {
        let recorded = record_replay::<C, S, E, _, _, _>(
            base_seed,
            snapshot_policy,
            frames,
            build,
            &self.replay_serializer,
        )?;

        self.build_fixture(recorded.artifact, recorded.result.final_snapshot)
    }

    pub fn encode(&self, fixture: &GoldenFixture<C, Snapshot>) -> Result<Vec<u8>, EngineError> {
        if fixture.metadata.artifact_schema_version != GOLDEN_FIXTURE_SCHEMA_VERSION {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "golden fixture",
                expected: GOLDEN_FIXTURE_SCHEMA_VERSION,
                got: fixture.metadata.artifact_schema_version,
            });
        }

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
        writer.push_str_hex("summary.engine_family_hex", &fixture.summary.engine_family);
        writer.push_display("summary.base_seed", fixture.summary.base_seed);
        writer.push_display("summary.final_tick", fixture.summary.final_tick);
        writer.push_display("summary.final_checksum", fixture.summary.final_checksum);
        writer.push_display(
            "summary.replay_artifact_schema_version",
            fixture.summary.replay_artifact_schema_version,
        );
        writer.push_display(
            "summary.snapshot_artifact_schema_version",
            fixture.summary.snapshot_artifact_schema_version,
        );
        writer.push_display(
            "summary.command_payload_schema_version",
            fixture.summary.command_payload_schema_version,
        );
        writer.push_display(
            "summary.snapshot_payload_schema_version",
            fixture.summary.snapshot_payload_schema_version,
        );
        writer.push_display("summary.replay_digest", fixture.summary.replay_digest);
        writer.push_display(
            "summary.snapshot_digest.present",
            encode_bool(fixture.summary.snapshot_digest.is_some()),
        );
        if let Some(snapshot_digest) = fixture.summary.snapshot_digest {
            writer.push_display("summary.snapshot_digest", snapshot_digest);
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

    pub fn decode(&self, bytes: &[u8]) -> Result<GoldenFixture<C, Snapshot>, EngineError> {
        let mut reader = CanonicalLineReader::new(bytes, "golden fixture").map_err(|error| {
            EngineError::CorruptedArtifact {
                artifact: "golden fixture",
                detail: error.to_string(),
            }
        })?;
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

        let summary = GoldenFixtureSummary {
            engine_family: decode_hex_string(
                read_fixture_value(&mut reader, "summary.engine_family_hex")?,
                "golden fixture summary engine family",
            )
            .map_err(golden_fixture_corrupted)?,
            base_seed: parse_u64(read_fixture_value(&mut reader, "summary.base_seed")?)?,
            final_tick: parse_u64(read_fixture_value(&mut reader, "summary.final_tick")?)?,
            final_checksum: parse_u64(read_fixture_value(&mut reader, "summary.final_checksum")?)?,
            replay_artifact_schema_version: parse_u32(read_fixture_value(
                &mut reader,
                "summary.replay_artifact_schema_version",
            )?)?,
            snapshot_artifact_schema_version: parse_u32(read_fixture_value(
                &mut reader,
                "summary.snapshot_artifact_schema_version",
            )?)?,
            command_payload_schema_version: parse_u32(read_fixture_value(
                &mut reader,
                "summary.command_payload_schema_version",
            )?)?,
            snapshot_payload_schema_version: parse_u32(read_fixture_value(
                &mut reader,
                "summary.snapshot_payload_schema_version",
            )?)?,
            replay_digest: parse_u64(read_fixture_value(&mut reader, "summary.replay_digest")?)?,
            snapshot_digest: {
                let present = parse_bool(read_fixture_value(
                    &mut reader,
                    "summary.snapshot_digest.present",
                )?)?;
                if present {
                    Some(parse_u64(read_fixture_value(
                        &mut reader,
                        "summary.snapshot_digest",
                    )?)?)
                } else {
                    None
                }
            },
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

        Ok(GoldenFixture {
            metadata: GoldenFixtureMetadata {
                artifact_schema_version,
            },
            replay_artifact,
            snapshot_artifact,
            summary,
        })
    }

    pub fn verify_fixture<S, E, Build>(
        &self,
        fixture: &GoldenFixture<C, Snapshot>,
        build: Build,
    ) -> Result<GoldenFixtureResult, EngineError>
    where
        S: SimulationState<Snapshot = Snapshot>,
        E: ReplayableEngine<C, State = S>,
        Build: Fn(Seed, SnapshotPolicy) -> E,
    {
        let frames = fixture
            .replay_artifact
            .records
            .iter()
            .map(|record| record.input.clone())
            .collect::<Vec<_>>();
        let recorded = record_replay::<C, S, E, _, _, _>(
            fixture.replay_artifact.metadata.base_seed,
            fixture.replay_artifact.metadata.snapshot_policy,
            frames.as_slice(),
            build,
            &self.replay_serializer,
        )?;
        let actual_summary = ParityArtifactSummary::from(&recorded.result.summary);
        let comparison = compare_parity_summaries(
            &ParityArtifactSummary::from(&fixture.summary),
            &actual_summary,
        );
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
            replay_mismatch,
            snapshot_mismatch,
        })
    }

    fn fixture_summary_from_artifacts(
        &self,
        replay_artifact: &ReplayArtifact<C, Snapshot>,
        snapshot_artifact: Option<&SnapshotArtifact<Snapshot>>,
    ) -> Result<GoldenFixtureSummary, EngineError> {
        let replay_summary = self.replay_serializer.summary(replay_artifact)?;
        let mut summary = GoldenFixtureSummary::from(&replay_summary);

        if let Some(snapshot_artifact) = snapshot_artifact {
            summary.snapshot_digest = Some(self.snapshot_serializer.digest(snapshot_artifact)?);
        }

        Ok(summary)
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
