use std::fmt;
use std::marker::PhantomData;

use crate::canonical::{
    CANONICAL_TEXT_ENCODING, CanonicalError, CanonicalLineReader, CanonicalLineWriter,
    canonical_digest, decode_hex, decode_hex_string, encode_hex,
};
use crate::core::{EngineError, Seed, Tick, tick_seed};
use crate::engine::{ReplayableEngine, SnapshotPolicy};
use crate::input::{Command, InputFrame};
use crate::replay::{
    ReplayTickRecord, SnapshotCaptureReason, SnapshotMetadata, SnapshotRecord,
    compare_replay_traces_with_snapshot_digest,
};
use crate::scheduler::PhaseGroup;
use crate::serialization::Serializer;
use crate::state::SimulationState;

pub const REPLAY_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const SNAPSHOT_ARTIFACT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayArtifactMetadata {
    pub artifact_schema_version: u32,
    pub engine_family: String,
    pub base_seed: Seed,
    pub total_ticks: Tick,
    pub snapshot_policy: SnapshotPolicy,
    pub command_payload_schema_version: u32,
    pub snapshot_payload_schema_version: u32,
    pub record_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayArtifact<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub metadata: ReplayArtifactMetadata,
    pub records: Vec<ReplayTickRecord<C, Snapshot>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotArtifactMetadata {
    pub artifact_schema_version: u32,
    pub engine_family: String,
    pub base_seed: Seed,
    pub capture_reason: SnapshotCaptureReason,
    pub snapshot: SnapshotMetadata,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotArtifact<Snapshot>
where
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub metadata: SnapshotArtifactMetadata,
    pub payload: Snapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactSummary {
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

impl ArtifactSummary {
    pub fn to_text(&self) -> String {
        format!(
            "engine_family={}\nbase_seed={}\nfinal_tick={}\nfinal_checksum={}\nreplay_artifact_schema_version={}\nsnapshot_artifact_schema_version={}\ncommand_payload_schema_version={}\nsnapshot_payload_schema_version={}\nreplay_digest={}\nsnapshot_digest={}\n",
            self.engine_family,
            self.base_seed,
            self.final_tick,
            self.final_checksum,
            self.replay_artifact_schema_version,
            self.snapshot_artifact_schema_version,
            self.command_payload_schema_version,
            self.snapshot_payload_schema_version,
            self.replay_digest,
            self.snapshot_digest
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".into())
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayExecutionMode {
    Record,
    ReplayVerify,
    ReplayFromSnapshot { source_tick: Tick },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayExecutionResult<Snapshot>
where
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub mode: ReplayExecutionMode,
    pub final_tick: Tick,
    pub final_checksum: u64,
    pub final_snapshot: Option<SnapshotArtifact<Snapshot>>,
    pub summary: ArtifactSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedReplay<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub artifact: ReplayArtifact<C, Snapshot>,
    pub result: ReplayExecutionResult<Snapshot>,
}

#[derive(Clone, Debug)]
pub struct SnapshotArtifactSerializer<Snapshot, PayloadSerializer> {
    engine_family: String,
    payload_serializer: PayloadSerializer,
    _marker: PhantomData<Snapshot>,
}

impl<Snapshot, PayloadSerializer> SnapshotArtifactSerializer<Snapshot, PayloadSerializer> {
    pub fn new(engine_family: impl Into<String>, payload_serializer: PayloadSerializer) -> Self {
        Self {
            engine_family: engine_family.into(),
            payload_serializer,
            _marker: PhantomData,
        }
    }

    pub fn build_artifact(
        &self,
        base_seed: Seed,
        record: &SnapshotRecord<Snapshot>,
    ) -> SnapshotArtifact<Snapshot>
    where
        Snapshot: Clone + Eq + fmt::Debug,
    {
        SnapshotArtifact {
            metadata: SnapshotArtifactMetadata {
                artifact_schema_version: SNAPSHOT_ARTIFACT_SCHEMA_VERSION,
                engine_family: self.engine_family.clone(),
                base_seed,
                capture_reason: record.reason.clone(),
                snapshot: record.metadata,
            },
            payload: record.payload.clone(),
        }
    }

    pub fn digest(&self, artifact: &SnapshotArtifact<Snapshot>) -> Result<u64, EngineError>
    where
        Snapshot: Clone + Eq + fmt::Debug,
        PayloadSerializer: Serializer<Snapshot>,
        PayloadSerializer::Error: fmt::Display,
    {
        self.encode(artifact).map(|bytes| canonical_digest(&bytes))
    }

    pub fn payload_digest(&self, payload: &Snapshot) -> Result<u64, EngineError>
    where
        Snapshot: Clone + Eq + fmt::Debug,
        PayloadSerializer: Serializer<Snapshot>,
        PayloadSerializer::Error: fmt::Display,
    {
        let bytes = self.payload_serializer.encode(payload).map_err(|error| {
            EngineError::SnapshotDecode {
                detail: format!("failed to digest snapshot payload: {error}"),
            }
        })?;
        Ok(canonical_digest(&bytes))
    }
}

impl<Snapshot, PayloadSerializer> Serializer<SnapshotArtifact<Snapshot>>
    for SnapshotArtifactSerializer<Snapshot, PayloadSerializer>
where
    Snapshot: Clone + Eq + fmt::Debug,
    PayloadSerializer: Serializer<Snapshot>,
    PayloadSerializer::Error: fmt::Display,
{
    type Error = EngineError;

    fn schema_version(&self) -> u32 {
        SNAPSHOT_ARTIFACT_SCHEMA_VERSION
    }

    fn encode(&self, value: &SnapshotArtifact<Snapshot>) -> Result<Vec<u8>, Self::Error> {
        encode_snapshot_artifact_bytes(
            &self.engine_family,
            &self.payload_serializer,
            self.schema_version(),
            value,
        )
    }

    fn decode(&self, bytes: &[u8]) -> Result<SnapshotArtifact<Snapshot>, Self::Error> {
        decode_snapshot_artifact_bytes(
            &self.engine_family,
            &self.payload_serializer,
            self.schema_version(),
            bytes,
        )
    }
}

#[derive(Clone, Debug)]
pub struct ReplayArtifactSerializer<C, Snapshot, CommandSerializer, SnapshotSerializer> {
    engine_family: String,
    command_serializer: CommandSerializer,
    snapshot_serializer: SnapshotSerializer,
    _marker: PhantomData<(C, Snapshot)>,
}

impl<C, Snapshot, CommandSerializer, SnapshotSerializer>
    ReplayArtifactSerializer<C, Snapshot, CommandSerializer, SnapshotSerializer>
{
    pub fn new(
        engine_family: impl Into<String>,
        command_serializer: CommandSerializer,
        snapshot_serializer: SnapshotSerializer,
    ) -> Self {
        Self {
            engine_family: engine_family.into(),
            command_serializer,
            snapshot_serializer,
            _marker: PhantomData,
        }
    }

    pub fn engine_family(&self) -> &str {
        &self.engine_family
    }
}

impl<C, Snapshot, CommandSerializer, SnapshotSerializer>
    ReplayArtifactSerializer<C, Snapshot, CommandSerializer, SnapshotSerializer>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
    CommandSerializer: Serializer<C>,
    SnapshotSerializer: Serializer<Snapshot>,
    CommandSerializer::Error: fmt::Display,
    SnapshotSerializer::Error: fmt::Display,
{
    pub fn build_artifact(
        &self,
        base_seed: Seed,
        snapshot_policy: SnapshotPolicy,
        records: &[ReplayTickRecord<C, Snapshot>],
    ) -> Result<ReplayArtifact<C, Snapshot>, EngineError> {
        validate_record_sequence(
            base_seed,
            snapshot_policy,
            self.snapshot_serializer.schema_version(),
            records,
        )?;

        Ok(ReplayArtifact {
            metadata: ReplayArtifactMetadata {
                artifact_schema_version: REPLAY_ARTIFACT_SCHEMA_VERSION,
                engine_family: self.engine_family.clone(),
                base_seed,
                total_ticks: records.last().map(|record| record.tick).unwrap_or(0),
                snapshot_policy,
                command_payload_schema_version: self.command_serializer.schema_version(),
                snapshot_payload_schema_version: self.snapshot_serializer.schema_version(),
                record_count: records.len(),
            },
            records: records.to_vec(),
        })
    }

    pub fn build_snapshot_artifact(
        &self,
        base_seed: Seed,
        record: &SnapshotRecord<Snapshot>,
    ) -> SnapshotArtifact<Snapshot> {
        SnapshotArtifact {
            metadata: SnapshotArtifactMetadata {
                artifact_schema_version: SNAPSHOT_ARTIFACT_SCHEMA_VERSION,
                engine_family: self.engine_family.clone(),
                base_seed,
                capture_reason: record.reason.clone(),
                snapshot: record.metadata,
            },
            payload: record.payload.clone(),
        }
    }

    pub fn digest(&self, artifact: &ReplayArtifact<C, Snapshot>) -> Result<u64, EngineError> {
        self.encode(artifact).map(|bytes| canonical_digest(&bytes))
    }

    pub fn summary(
        &self,
        artifact: &ReplayArtifact<C, Snapshot>,
    ) -> Result<ArtifactSummary, EngineError> {
        let replay_digest = self.digest(artifact)?;
        let final_checksum = artifact
            .records
            .last()
            .map(|record| record.checksum)
            .unwrap_or(0);
        let final_tick = artifact.metadata.total_ticks;
        let snapshot_digest = artifact
            .records
            .iter()
            .rev()
            .find_map(|record| record.snapshot.as_ref())
            .map(|record| self.build_snapshot_artifact(artifact.metadata.base_seed, record))
            .map(|artifact| {
                encode_snapshot_artifact_bytes(
                    &self.engine_family,
                    &self.snapshot_serializer,
                    SNAPSHOT_ARTIFACT_SCHEMA_VERSION,
                    &artifact,
                )
                .map(|bytes| canonical_digest(&bytes))
            })
            .transpose()?;

        Ok(ArtifactSummary {
            engine_family: artifact.metadata.engine_family.clone(),
            base_seed: artifact.metadata.base_seed,
            final_tick,
            final_checksum,
            replay_artifact_schema_version: artifact.metadata.artifact_schema_version,
            snapshot_artifact_schema_version: SNAPSHOT_ARTIFACT_SCHEMA_VERSION,
            command_payload_schema_version: artifact.metadata.command_payload_schema_version,
            snapshot_payload_schema_version: artifact.metadata.snapshot_payload_schema_version,
            replay_digest,
            snapshot_digest,
        })
    }

    fn snapshot_payload_digest(&self, snapshot: &Snapshot) -> Result<u64, EngineError> {
        let bytes = self.snapshot_serializer.encode(snapshot).map_err(|error| {
            EngineError::ReplayMismatch {
                tick: None,
                detail: format!("failed to digest snapshot payload: {error}"),
            }
        })?;
        Ok(canonical_digest(&bytes))
    }
}

impl<C, Snapshot, CommandSerializer, SnapshotSerializer> Serializer<ReplayArtifact<C, Snapshot>>
    for ReplayArtifactSerializer<C, Snapshot, CommandSerializer, SnapshotSerializer>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
    CommandSerializer: Serializer<C>,
    SnapshotSerializer: Serializer<Snapshot>,
    CommandSerializer::Error: fmt::Display,
    SnapshotSerializer::Error: fmt::Display,
{
    type Error = EngineError;

    fn schema_version(&self) -> u32 {
        REPLAY_ARTIFACT_SCHEMA_VERSION
    }

    fn encode(&self, value: &ReplayArtifact<C, Snapshot>) -> Result<Vec<u8>, Self::Error> {
        if value.metadata.artifact_schema_version != self.schema_version() {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "replay artifact",
                expected: self.schema_version(),
                got: value.metadata.artifact_schema_version,
            });
        }

        if value.metadata.engine_family != self.engine_family {
            return Err(EngineError::ReplayDecode {
                detail: format!(
                    "engine family mismatch: expected `{}`, got `{}`",
                    self.engine_family, value.metadata.engine_family
                ),
            });
        }

        if value.metadata.command_payload_schema_version != self.command_serializer.schema_version()
        {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "replay command payload",
                expected: self.command_serializer.schema_version(),
                got: value.metadata.command_payload_schema_version,
            });
        }

        if value.metadata.snapshot_payload_schema_version
            != self.snapshot_serializer.schema_version()
        {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "replay snapshot payload",
                expected: self.snapshot_serializer.schema_version(),
                got: value.metadata.snapshot_payload_schema_version,
            });
        }

        validate_record_sequence(
            value.metadata.base_seed,
            value.metadata.snapshot_policy,
            value.metadata.snapshot_payload_schema_version,
            &value.records,
        )?;

        let mut writer = CanonicalLineWriter::default();
        writer.push_display("artifact", "replay");
        writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
        writer.push_display(
            "artifact_schema_version",
            value.metadata.artifact_schema_version,
        );
        writer.push_str_hex("engine_family_hex", &value.metadata.engine_family);
        writer.push_display("base_seed", value.metadata.base_seed);
        writer.push_display("total_ticks", value.metadata.total_ticks);
        writer.push_display(
            "snapshot_policy",
            encode_snapshot_policy(value.metadata.snapshot_policy),
        );
        writer.push_display(
            "command_payload_schema_version",
            value.metadata.command_payload_schema_version,
        );
        writer.push_display(
            "snapshot_payload_schema_version",
            value.metadata.snapshot_payload_schema_version,
        );
        writer.push_display("record_count", value.metadata.record_count);

        for (index, record) in value.records.iter().enumerate() {
            let command_bytes = self
                .command_serializer
                .encode(&record.input.command)
                .map_err(|error| EngineError::ReplayDecode {
                    detail: format!("failed to encode command at tick {}: {error}", record.tick),
                })?;

            writer.push_display(format!("record.{index}.tick").as_str(), record.tick);
            writer.push_display(
                format!("record.{index}.input.tick").as_str(),
                record.input.tick,
            );
            writer.push_display(
                format!("record.{index}.command_hex").as_str(),
                encode_hex(&command_bytes),
            );
            writer.push_display(
                format!("record.{index}.tick_seed").as_str(),
                record.tick_seed,
            );
            writer.push_display(
                format!("record.{index}.phase_count").as_str(),
                record.phase_markers.len(),
            );

            for (phase_index, marker) in record.phase_markers.iter().enumerate() {
                writer.push_display(
                    format!("record.{index}.phase.{phase_index}.ordinal").as_str(),
                    marker.ordinal,
                );
                writer.push_display(
                    format!("record.{index}.phase.{phase_index}.group").as_str(),
                    marker.group.as_str(),
                );
                writer.push_str_hex(
                    format!("record.{index}.phase.{phase_index}.name_hex").as_str(),
                    &marker.name,
                );
            }

            writer.push_display(format!("record.{index}.checksum").as_str(), record.checksum);
            writer.push_display(
                format!("record.{index}.snapshot.present").as_str(),
                encode_bool(record.snapshot.is_some()),
            );

            if let Some(snapshot) = &record.snapshot {
                let payload_bytes =
                    self.snapshot_serializer
                        .encode(&snapshot.payload)
                        .map_err(|error| EngineError::SnapshotSerialization {
                            tick: snapshot.metadata.source_tick,
                            reason: error.to_string(),
                        })?;

                writer.push_display(
                    format!("record.{index}.snapshot.reason").as_str(),
                    encode_snapshot_capture_reason(&snapshot.reason),
                );
                writer.push_display(
                    format!("record.{index}.snapshot.payload_schema_version").as_str(),
                    snapshot.metadata.payload_schema_version,
                );
                writer.push_display(
                    format!("record.{index}.snapshot.source_tick").as_str(),
                    snapshot.metadata.source_tick,
                );
                writer.push_display(
                    format!("record.{index}.snapshot.capture_checksum").as_str(),
                    snapshot.metadata.capture_checksum,
                );
                writer.push_display(
                    format!("record.{index}.snapshot.payload_hex").as_str(),
                    encode_hex(&payload_bytes),
                );
            }
        }

        Ok(writer.finish())
    }

    fn decode(&self, bytes: &[u8]) -> Result<ReplayArtifact<C, Snapshot>, Self::Error> {
        let mut reader = CanonicalLineReader::new(bytes, "replay artifact")
            .map_err(replay_corrupted_canonical)?;
        expect_replay_value(&mut reader, "artifact", "replay")?;
        expect_replay_value(&mut reader, "canonical_encoding", CANONICAL_TEXT_ENCODING)?;

        let artifact_schema_version = parse_u32(
            read_replay_value(&mut reader, "artifact_schema_version")?,
            "replay",
            "artifact schema version",
        )?;
        if artifact_schema_version != self.schema_version() {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "replay artifact",
                expected: self.schema_version(),
                got: artifact_schema_version,
            });
        }

        let engine_family = decode_hex_string(
            read_replay_value(&mut reader, "engine_family_hex")?,
            "replay artifact engine family",
        )
        .map_err(replay_corrupted_canonical)?;
        if engine_family != self.engine_family {
            return Err(EngineError::ReplayDecode {
                detail: format!(
                    "engine family mismatch: expected `{}`, got `{engine_family}`",
                    self.engine_family
                ),
            });
        }

        let base_seed = parse_u64(
            read_replay_value(&mut reader, "base_seed")?,
            "replay",
            "base seed",
        )?;
        let total_ticks = parse_u64(
            read_replay_value(&mut reader, "total_ticks")?,
            "replay",
            "total ticks",
        )?;
        let snapshot_policy =
            parse_snapshot_policy(read_replay_value(&mut reader, "snapshot_policy")?)?;
        let command_payload_schema_version = parse_u32(
            read_replay_value(&mut reader, "command_payload_schema_version")?,
            "replay",
            "command payload schema version",
        )?;
        if command_payload_schema_version != self.command_serializer.schema_version() {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "replay command payload",
                expected: self.command_serializer.schema_version(),
                got: command_payload_schema_version,
            });
        }

        let snapshot_payload_schema_version = parse_u32(
            read_replay_value(&mut reader, "snapshot_payload_schema_version")?,
            "replay",
            "snapshot payload schema version",
        )?;
        if snapshot_payload_schema_version != self.snapshot_serializer.schema_version() {
            return Err(EngineError::UnsupportedSchemaVersion {
                artifact: "replay snapshot payload",
                expected: self.snapshot_serializer.schema_version(),
                got: snapshot_payload_schema_version,
            });
        }

        let record_count = parse_usize(
            read_replay_value(&mut reader, "record_count")?,
            "replay",
            "record count",
        )?;
        let mut records = Vec::with_capacity(record_count);

        for index in 0..record_count {
            let tick = parse_u64(
                read_replay_value(&mut reader, format!("record.{index}.tick").as_str())?,
                "replay",
                "record tick",
            )?;
            let input_tick = parse_u64(
                read_replay_value(&mut reader, format!("record.{index}.input.tick").as_str())?,
                "replay",
                "record input tick",
            )?;
            let command_hex =
                read_replay_value(&mut reader, format!("record.{index}.command_hex").as_str())?;
            let command_bytes = decode_hex(command_hex, "replay artifact command")
                .map_err(replay_corrupted_canonical)?;
            let command = self
                .command_serializer
                .decode(&command_bytes)
                .map_err(|error| EngineError::ReplayDecode {
                    detail: format!("failed to decode command at tick {tick}: {error}"),
                })?;
            let tick_seed_value = parse_u64(
                read_replay_value(&mut reader, format!("record.{index}.tick_seed").as_str())?,
                "replay",
                "record tick seed",
            )?;
            let phase_count = parse_usize(
                read_replay_value(&mut reader, format!("record.{index}.phase_count").as_str())?,
                "replay",
                "phase count",
            )?;
            let mut phase_markers = Vec::with_capacity(phase_count);

            for phase_index in 0..phase_count {
                let ordinal = parse_usize(
                    read_replay_value(
                        &mut reader,
                        format!("record.{index}.phase.{phase_index}.ordinal").as_str(),
                    )?,
                    "replay",
                    "phase ordinal",
                )?;
                let group = PhaseGroup::parse(read_replay_value(
                    &mut reader,
                    format!("record.{index}.phase.{phase_index}.group").as_str(),
                )?)
                .ok_or_else(|| EngineError::CorruptedArtifact {
                    artifact: "replay",
                    detail: format!("invalid phase group at record {index}, phase {phase_index}"),
                })?;
                let name = decode_hex_string(
                    read_replay_value(
                        &mut reader,
                        format!("record.{index}.phase.{phase_index}.name_hex").as_str(),
                    )?,
                    "replay artifact phase name",
                )
                .map_err(replay_corrupted_canonical)?;
                phase_markers.push(crate::replay::PhaseMarker {
                    ordinal,
                    group,
                    name,
                });
            }

            let checksum = parse_u64(
                read_replay_value(&mut reader, format!("record.{index}.checksum").as_str())?,
                "replay",
                "record checksum",
            )?;
            let snapshot_present = parse_bool(read_replay_value(
                &mut reader,
                format!("record.{index}.snapshot.present").as_str(),
            )?)?;

            let snapshot = if snapshot_present {
                let reason = parse_snapshot_capture_reason(read_replay_value(
                    &mut reader,
                    format!("record.{index}.snapshot.reason").as_str(),
                )?)?;
                let payload_schema_version = parse_u32(
                    read_replay_value(
                        &mut reader,
                        format!("record.{index}.snapshot.payload_schema_version").as_str(),
                    )?,
                    "replay",
                    "snapshot payload schema version",
                )?;
                let source_tick = parse_u64(
                    read_replay_value(
                        &mut reader,
                        format!("record.{index}.snapshot.source_tick").as_str(),
                    )?,
                    "replay",
                    "snapshot source tick",
                )?;
                let capture_checksum = parse_u64(
                    read_replay_value(
                        &mut reader,
                        format!("record.{index}.snapshot.capture_checksum").as_str(),
                    )?,
                    "replay",
                    "snapshot capture checksum",
                )?;
                let payload_hex = read_replay_value(
                    &mut reader,
                    format!("record.{index}.snapshot.payload_hex").as_str(),
                )?;
                let payload_bytes = decode_hex(payload_hex, "replay artifact snapshot payload")
                    .map_err(replay_corrupted_canonical)?;
                let payload = self
                    .snapshot_serializer
                    .decode(&payload_bytes)
                    .map_err(|error| EngineError::ReplayDecode {
                        detail: format!(
                            "failed to decode snapshot payload at tick {source_tick}: {error}"
                        ),
                    })?;

                Some(SnapshotRecord {
                    metadata: SnapshotMetadata {
                        payload_schema_version,
                        source_tick,
                        capture_checksum,
                    },
                    reason,
                    payload,
                })
            } else {
                None
            };

            records.push(ReplayTickRecord {
                tick,
                input: InputFrame::new(input_tick, command),
                tick_seed: tick_seed_value,
                phase_markers,
                checksum,
                snapshot,
            });
        }

        reader
            .finish("replay artifact")
            .map_err(replay_corrupted_canonical)?;

        let artifact = ReplayArtifact {
            metadata: ReplayArtifactMetadata {
                artifact_schema_version,
                engine_family,
                base_seed,
                total_ticks,
                snapshot_policy,
                command_payload_schema_version,
                snapshot_payload_schema_version,
                record_count,
            },
            records,
        };

        validate_replay_artifact_shape(&artifact)?;
        Ok(artifact)
    }
}

pub fn validate_snapshot_artifact<S>(
    artifact: &SnapshotArtifact<S::Snapshot>,
    expected_seed: Seed,
) -> Result<(), EngineError>
where
    S: SimulationState,
{
    if artifact.metadata.base_seed != expected_seed {
        return Err(EngineError::SeedMismatch {
            expected: expected_seed,
            got: artifact.metadata.base_seed,
        });
    }

    if artifact.metadata.snapshot.payload_schema_version != S::snapshot_schema_version() {
        return Err(EngineError::UnsupportedSchemaVersion {
            artifact: "snapshot payload",
            expected: S::snapshot_schema_version(),
            got: artifact.metadata.snapshot.payload_schema_version,
        });
    }

    let payload_tick = S::snapshot_tick(&artifact.payload);
    if artifact.metadata.snapshot.source_tick != payload_tick {
        return Err(EngineError::SnapshotMetadataMismatch {
            detail: format!(
                "source tick mismatch: metadata={}, payload={payload_tick}",
                artifact.metadata.snapshot.source_tick
            ),
        });
    }

    let payload_checksum = S::snapshot_checksum(&artifact.payload);
    if artifact.metadata.snapshot.capture_checksum != payload_checksum {
        return Err(EngineError::SnapshotMetadataMismatch {
            detail: format!(
                "capture checksum mismatch: metadata={}, payload={payload_checksum}",
                artifact.metadata.snapshot.capture_checksum
            ),
        });
    }

    Ok(())
}

pub fn record_replay<C, S, E, Build, CommandSerializer, SnapshotSerializer>(
    base_seed: Seed,
    snapshot_policy: SnapshotPolicy,
    frames: &[InputFrame<C>],
    build: Build,
    serializer: &ReplayArtifactSerializer<C, S::Snapshot, CommandSerializer, SnapshotSerializer>,
) -> Result<RecordedReplay<C, S::Snapshot>, EngineError>
where
    C: Command,
    S: SimulationState,
    E: ReplayableEngine<C, State = S>,
    Build: Fn(Seed, SnapshotPolicy) -> E,
    CommandSerializer: Serializer<C>,
    SnapshotSerializer: Serializer<S::Snapshot>,
    CommandSerializer::Error: fmt::Display,
    SnapshotSerializer::Error: fmt::Display,
{
    let mut engine = build(base_seed, snapshot_policy);
    if engine.seed() != base_seed {
        return Err(EngineError::SeedMismatch {
            expected: base_seed,
            got: engine.seed(),
        });
    }

    for frame in frames {
        engine.tick(frame.clone())?;
    }

    let artifact =
        serializer.build_artifact(base_seed, snapshot_policy, engine.replay_records())?;
    let summary = serializer.summary(&artifact)?;
    let final_tick = artifact.metadata.total_ticks;
    let final_checksum = artifact
        .records
        .last()
        .map(|record| record.checksum)
        .unwrap_or(0);
    let final_snapshot = artifact
        .records
        .iter()
        .rev()
        .find_map(|record| record.snapshot.as_ref())
        .map(|record| serializer.build_snapshot_artifact(base_seed, record));

    Ok(RecordedReplay {
        artifact,
        result: ReplayExecutionResult {
            mode: ReplayExecutionMode::Record,
            final_tick,
            final_checksum,
            final_snapshot,
            summary,
        },
    })
}

pub fn execute_replay_verify<C, S, E, Build, CommandSerializer, SnapshotSerializer>(
    artifact: &ReplayArtifact<C, S::Snapshot>,
    build: Build,
    serializer: &ReplayArtifactSerializer<C, S::Snapshot, CommandSerializer, SnapshotSerializer>,
) -> Result<ReplayExecutionResult<S::Snapshot>, EngineError>
where
    C: Command,
    S: SimulationState,
    E: ReplayableEngine<C, State = S>,
    Build: Fn(Seed, SnapshotPolicy) -> E,
    CommandSerializer: Serializer<C>,
    SnapshotSerializer: Serializer<S::Snapshot>,
    CommandSerializer::Error: fmt::Display,
    SnapshotSerializer::Error: fmt::Display,
{
    validate_replay_artifact_for_state::<C, S>(artifact)?;

    let mut engine = build(
        artifact.metadata.base_seed,
        artifact.metadata.snapshot_policy,
    );
    if engine.seed() != artifact.metadata.base_seed {
        return Err(EngineError::SeedMismatch {
            expected: artifact.metadata.base_seed,
            got: engine.seed(),
        });
    }

    for record in &artifact.records {
        engine.tick(record.input.clone())?;
    }

    compare_replay_traces_with_snapshot_digest(
        artifact.records.as_slice(),
        engine.replay_records(),
        |snapshot| serializer.snapshot_payload_digest(snapshot),
    )?;
    let summary = serializer.summary(artifact)?;
    let final_tick = artifact.metadata.total_ticks;
    let final_checksum = artifact
        .records
        .last()
        .map(|record| record.checksum)
        .unwrap_or(0);
    let final_snapshot = artifact
        .records
        .iter()
        .rev()
        .find_map(|record| record.snapshot.as_ref())
        .map(|record| serializer.build_snapshot_artifact(artifact.metadata.base_seed, record));

    Ok(ReplayExecutionResult {
        mode: ReplayExecutionMode::ReplayVerify,
        final_tick,
        final_checksum,
        final_snapshot,
        summary,
    })
}

pub fn execute_replay_from_snapshot<C, S, E, Build, CommandSerializer, SnapshotSerializer>(
    snapshot: &SnapshotArtifact<S::Snapshot>,
    artifact: &ReplayArtifact<C, S::Snapshot>,
    build: Build,
    serializer: &ReplayArtifactSerializer<C, S::Snapshot, CommandSerializer, SnapshotSerializer>,
) -> Result<ReplayExecutionResult<S::Snapshot>, EngineError>
where
    C: Command,
    S: SimulationState,
    E: ReplayableEngine<C, State = S>,
    Build: Fn(Seed, SnapshotPolicy) -> E,
    CommandSerializer: Serializer<C>,
    SnapshotSerializer: Serializer<S::Snapshot>,
    CommandSerializer::Error: fmt::Display,
    SnapshotSerializer::Error: fmt::Display,
{
    if snapshot.metadata.engine_family != artifact.metadata.engine_family {
        return Err(EngineError::SnapshotMetadataMismatch {
            detail: format!(
                "snapshot engine family `{}` does not match replay engine family `{}`",
                snapshot.metadata.engine_family, artifact.metadata.engine_family
            ),
        });
    }

    validate_snapshot_artifact::<S>(snapshot, artifact.metadata.base_seed)?;

    let source_tick = snapshot.metadata.snapshot.source_tick;
    let source_index = artifact
        .records
        .iter()
        .position(|record| record.tick == source_tick)
        .ok_or_else(|| EngineError::ReplayContinuationMismatch {
            detail: format!("snapshot tick {source_tick} not found in replay trace"),
        })?;

    let source_record = &artifact.records[source_index];
    let expected_snapshot =
        source_record
            .snapshot
            .as_ref()
            .ok_or_else(|| EngineError::ReplayContinuationMismatch {
                detail: format!("replay tick {source_tick} does not contain a snapshot"),
            })?;

    if expected_snapshot.reason != snapshot.metadata.capture_reason
        || expected_snapshot.metadata != snapshot.metadata.snapshot
        || expected_snapshot.payload != snapshot.payload
    {
        return Err(EngineError::SnapshotMetadataMismatch {
            detail: format!(
                "snapshot artifact does not match replay snapshot recorded at tick {source_tick}"
            ),
        });
    }

    let continuation = &artifact.records[source_index + 1..];
    if let Some(first) = continuation.first() {
        let expected_tick = source_tick.saturating_add(1);
        if first.tick != expected_tick {
            return Err(EngineError::ResumeTickMismatch {
                expected: expected_tick,
                got: first.tick,
            });
        }
    }

    validate_replay_artifact_for_state::<C, S>(artifact)?;

    let mut engine = build(
        artifact.metadata.base_seed,
        artifact.metadata.snapshot_policy,
    );
    if engine.seed() != artifact.metadata.base_seed {
        return Err(EngineError::SeedMismatch {
            expected: artifact.metadata.base_seed,
            got: engine.seed(),
        });
    }

    engine.restore_snapshot(snapshot.payload.clone());
    if engine.state().tick() != source_tick {
        return Err(EngineError::SnapshotMetadataMismatch {
            detail: format!(
                "restored engine tick mismatch: expected {source_tick}, got {}",
                engine.state().tick()
            ),
        });
    }

    for record in continuation {
        engine.tick(record.input.clone())?;
    }

    compare_replay_traces_with_snapshot_digest(
        continuation,
        engine.replay_records(),
        |snapshot| serializer.snapshot_payload_digest(snapshot),
    )?;
    let summary = serializer.summary(artifact)?;
    let final_tick = continuation
        .last()
        .map(|record| record.tick)
        .unwrap_or(source_tick);
    let final_checksum = continuation
        .last()
        .map(|record| record.checksum)
        .unwrap_or(snapshot.metadata.snapshot.capture_checksum);
    let final_snapshot = continuation
        .iter()
        .rev()
        .find_map(|record| record.snapshot.as_ref())
        .map(|record| serializer.build_snapshot_artifact(artifact.metadata.base_seed, record))
        .or_else(|| Some(snapshot.clone()));

    Ok(ReplayExecutionResult {
        mode: ReplayExecutionMode::ReplayFromSnapshot { source_tick },
        final_tick,
        final_checksum,
        final_snapshot,
        summary,
    })
}

fn validate_replay_artifact_for_state<C, S>(
    artifact: &ReplayArtifact<C, S::Snapshot>,
) -> Result<(), EngineError>
where
    C: Command,
    S: SimulationState,
{
    if artifact.metadata.snapshot_payload_schema_version != S::snapshot_schema_version() {
        return Err(EngineError::UnsupportedSchemaVersion {
            artifact: "replay snapshot payload",
            expected: S::snapshot_schema_version(),
            got: artifact.metadata.snapshot_payload_schema_version,
        });
    }

    validate_replay_artifact_shape(artifact)?;

    for record in &artifact.records {
        if let Some(snapshot) = &record.snapshot {
            let payload_tick = S::snapshot_tick(&snapshot.payload);
            if snapshot.metadata.source_tick != payload_tick {
                return Err(EngineError::SnapshotMetadataMismatch {
                    detail: format!(
                        "snapshot payload tick mismatch at replay tick {}: metadata={}, payload={payload_tick}",
                        record.tick, snapshot.metadata.source_tick
                    ),
                });
            }

            let payload_checksum = S::snapshot_checksum(&snapshot.payload);
            if snapshot.metadata.capture_checksum != payload_checksum {
                return Err(EngineError::SnapshotMetadataMismatch {
                    detail: format!(
                        "snapshot checksum mismatch at replay tick {}: metadata={}, payload={payload_checksum}",
                        record.tick, snapshot.metadata.capture_checksum
                    ),
                });
            }
        }
    }

    Ok(())
}

fn validate_replay_artifact_shape<C, Snapshot>(
    artifact: &ReplayArtifact<C, Snapshot>,
) -> Result<(), EngineError>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    validate_record_sequence(
        artifact.metadata.base_seed,
        artifact.metadata.snapshot_policy,
        artifact.metadata.snapshot_payload_schema_version,
        &artifact.records,
    )?;

    if artifact.metadata.record_count != artifact.records.len() {
        return Err(EngineError::CorruptedArtifact {
            artifact: "replay",
            detail: format!(
                "record count mismatch: metadata={}, actual={}",
                artifact.metadata.record_count,
                artifact.records.len()
            ),
        });
    }

    let expected_total_ticks = artifact
        .records
        .last()
        .map(|record| record.tick)
        .unwrap_or(0);
    if artifact.metadata.total_ticks != expected_total_ticks {
        return Err(EngineError::CorruptedArtifact {
            artifact: "replay",
            detail: format!(
                "total tick mismatch: metadata={}, actual={expected_total_ticks}",
                artifact.metadata.total_ticks
            ),
        });
    }

    Ok(())
}

fn validate_record_sequence<C, Snapshot>(
    base_seed: Seed,
    snapshot_policy: SnapshotPolicy,
    expected_snapshot_payload_schema_version: u32,
    records: &[ReplayTickRecord<C, Snapshot>],
) -> Result<(), EngineError>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    let mut expected_tick = 1;

    for record in records {
        if record.tick != expected_tick {
            return Err(EngineError::CorruptedArtifact {
                artifact: "replay",
                detail: format!(
                    "non-contiguous tick sequence: expected {expected_tick}, got {}",
                    record.tick
                ),
            });
        }

        if record.input.tick != record.tick {
            return Err(EngineError::CorruptedArtifact {
                artifact: "replay",
                detail: format!(
                    "input tick mismatch at replay tick {}: input tick {}",
                    record.tick, record.input.tick
                ),
            });
        }

        let expected_seed = tick_seed(base_seed, record.tick);
        if record.tick_seed != expected_seed {
            return Err(EngineError::CorruptedArtifact {
                artifact: "replay",
                detail: format!(
                    "tick seed mismatch at tick {}: expected {expected_seed}, got {}",
                    record.tick, record.tick_seed
                ),
            });
        }

        match (&snapshot_policy, &record.snapshot) {
            (SnapshotPolicy::Never, Some(_)) => {
                return Err(EngineError::CorruptedArtifact {
                    artifact: "replay",
                    detail: format!(
                        "snapshot recorded at tick {} while snapshot policy is Never",
                        record.tick
                    ),
                });
            }
            (SnapshotPolicy::Manual, Some(_)) => {
                return Err(EngineError::CorruptedArtifact {
                    artifact: "replay",
                    detail: format!(
                        "policy replay snapshot recorded at tick {} while snapshot policy is Manual",
                        record.tick
                    ),
                });
            }
            (SnapshotPolicy::Every { interval }, None)
                if *interval != 0 && record.tick % interval == 0 =>
            {
                return Err(EngineError::CorruptedArtifact {
                    artifact: "replay",
                    detail: format!(
                        "missing snapshot at tick {} for every-{interval} policy",
                        record.tick
                    ),
                });
            }
            (SnapshotPolicy::Every { interval }, Some(snapshot)) => {
                if *interval == 0 || record.tick % interval != 0 {
                    return Err(EngineError::CorruptedArtifact {
                        artifact: "replay",
                        detail: format!(
                            "unexpected snapshot at tick {} for every-{interval} policy",
                            record.tick
                        ),
                    });
                }

                if snapshot.reason
                    != (SnapshotCaptureReason::PolicyInterval {
                        interval: *interval,
                    })
                {
                    return Err(EngineError::CorruptedArtifact {
                        artifact: "replay",
                        detail: format!(
                            "snapshot reason mismatch at tick {} for every-{interval} policy",
                            record.tick
                        ),
                    });
                }
            }
            _ => {}
        }

        if let Some(snapshot) = &record.snapshot {
            if snapshot.metadata.payload_schema_version != expected_snapshot_payload_schema_version
            {
                return Err(EngineError::UnsupportedSchemaVersion {
                    artifact: "replay snapshot payload",
                    expected: expected_snapshot_payload_schema_version,
                    got: snapshot.metadata.payload_schema_version,
                });
            }

            if snapshot.metadata.source_tick != record.tick {
                return Err(EngineError::CorruptedArtifact {
                    artifact: "replay",
                    detail: format!(
                        "snapshot source tick mismatch at tick {}: got {}",
                        record.tick, snapshot.metadata.source_tick
                    ),
                });
            }

            if snapshot.metadata.capture_checksum != record.checksum {
                return Err(EngineError::CorruptedArtifact {
                    artifact: "replay",
                    detail: format!(
                        "snapshot checksum mismatch at tick {}: snapshot={}, record={}",
                        record.tick, snapshot.metadata.capture_checksum, record.checksum
                    ),
                });
            }
        }

        expected_tick += 1;
    }

    Ok(())
}

fn encode_snapshot_artifact_bytes<Snapshot, PayloadSerializer>(
    engine_family: &str,
    payload_serializer: &PayloadSerializer,
    artifact_schema_version: u32,
    value: &SnapshotArtifact<Snapshot>,
) -> Result<Vec<u8>, EngineError>
where
    Snapshot: Clone + Eq + fmt::Debug,
    PayloadSerializer: Serializer<Snapshot>,
    PayloadSerializer::Error: fmt::Display,
{
    if value.metadata.artifact_schema_version != artifact_schema_version {
        return Err(EngineError::UnsupportedSchemaVersion {
            artifact: "snapshot artifact",
            expected: artifact_schema_version,
            got: value.metadata.artifact_schema_version,
        });
    }

    if value.metadata.engine_family != engine_family {
        return Err(EngineError::SnapshotMetadataMismatch {
            detail: format!(
                "engine family mismatch: expected `{engine_family}`, got `{}`",
                value.metadata.engine_family
            ),
        });
    }

    if value.metadata.snapshot.payload_schema_version != payload_serializer.schema_version() {
        return Err(EngineError::UnsupportedSchemaVersion {
            artifact: "snapshot payload",
            expected: payload_serializer.schema_version(),
            got: value.metadata.snapshot.payload_schema_version,
        });
    }

    let payload = payload_serializer.encode(&value.payload).map_err(|error| {
        EngineError::SnapshotSerialization {
            tick: value.metadata.snapshot.source_tick,
            reason: error.to_string(),
        }
    })?;

    let mut writer = CanonicalLineWriter::default();
    writer.push_display("artifact", "snapshot");
    writer.push_display("canonical_encoding", CANONICAL_TEXT_ENCODING);
    writer.push_display(
        "artifact_schema_version",
        value.metadata.artifact_schema_version,
    );
    writer.push_str_hex("engine_family_hex", &value.metadata.engine_family);
    writer.push_display("base_seed", value.metadata.base_seed);
    writer.push_display(
        "capture_reason",
        encode_snapshot_capture_reason(&value.metadata.capture_reason),
    );
    writer.push_display(
        "snapshot_payload_schema_version",
        value.metadata.snapshot.payload_schema_version,
    );
    writer.push_display("source_tick", value.metadata.snapshot.source_tick);
    writer.push_display("capture_checksum", value.metadata.snapshot.capture_checksum);
    writer.push_display("payload_hex", encode_hex(&payload));
    Ok(writer.finish())
}

fn decode_snapshot_artifact_bytes<Snapshot, PayloadSerializer>(
    engine_family: &str,
    payload_serializer: &PayloadSerializer,
    artifact_schema_version: u32,
    bytes: &[u8],
) -> Result<SnapshotArtifact<Snapshot>, EngineError>
where
    Snapshot: Clone + Eq + fmt::Debug,
    PayloadSerializer: Serializer<Snapshot>,
    PayloadSerializer::Error: fmt::Display,
{
    let mut reader = CanonicalLineReader::new(bytes, "snapshot artifact")
        .map_err(snapshot_corrupted_canonical)?;
    expect_snapshot_value(&mut reader, "artifact", "snapshot")?;
    expect_snapshot_value(&mut reader, "canonical_encoding", CANONICAL_TEXT_ENCODING)?;

    let decoded_artifact_schema_version = parse_u32(
        read_snapshot_value(&mut reader, "artifact_schema_version")?,
        "snapshot",
        "artifact schema version",
    )?;
    if decoded_artifact_schema_version != artifact_schema_version {
        return Err(EngineError::UnsupportedSchemaVersion {
            artifact: "snapshot artifact",
            expected: artifact_schema_version,
            got: decoded_artifact_schema_version,
        });
    }

    let decoded_engine_family = decode_hex_string(
        read_snapshot_value(&mut reader, "engine_family_hex")?,
        "snapshot artifact engine family",
    )
    .map_err(snapshot_corrupted_canonical)?;
    if decoded_engine_family != engine_family {
        return Err(EngineError::SnapshotDecode {
            detail: format!(
                "engine family mismatch: expected `{engine_family}`, got `{decoded_engine_family}`"
            ),
        });
    }

    let base_seed = parse_u64(
        read_snapshot_value(&mut reader, "base_seed")?,
        "snapshot",
        "base seed",
    )?;
    let capture_reason =
        parse_snapshot_capture_reason(read_snapshot_value(&mut reader, "capture_reason")?)?;
    let payload_schema_version = parse_u32(
        read_snapshot_value(&mut reader, "snapshot_payload_schema_version")?,
        "snapshot",
        "snapshot payload schema version",
    )?;
    if payload_schema_version != payload_serializer.schema_version() {
        return Err(EngineError::UnsupportedSchemaVersion {
            artifact: "snapshot payload",
            expected: payload_serializer.schema_version(),
            got: payload_schema_version,
        });
    }

    let source_tick = parse_u64(
        read_snapshot_value(&mut reader, "source_tick")?,
        "snapshot",
        "source tick",
    )?;
    let capture_checksum = parse_u64(
        read_snapshot_value(&mut reader, "capture_checksum")?,
        "snapshot",
        "capture checksum",
    )?;
    let payload_hex = read_snapshot_value(&mut reader, "payload_hex")?;
    let payload_bytes = decode_hex(payload_hex, "snapshot artifact payload")
        .map_err(snapshot_corrupted_canonical)?;
    let payload =
        payload_serializer
            .decode(&payload_bytes)
            .map_err(|error| EngineError::SnapshotDecode {
                detail: error.to_string(),
            })?;

    reader
        .finish("snapshot artifact")
        .map_err(snapshot_corrupted_canonical)?;

    Ok(SnapshotArtifact {
        metadata: SnapshotArtifactMetadata {
            artifact_schema_version: decoded_artifact_schema_version,
            engine_family: decoded_engine_family,
            base_seed,
            capture_reason,
            snapshot: SnapshotMetadata {
                payload_schema_version,
                source_tick,
                capture_checksum,
            },
        },
        payload,
    })
}

fn encode_snapshot_policy(policy: SnapshotPolicy) -> String {
    match policy {
        SnapshotPolicy::Never => "never".into(),
        SnapshotPolicy::Every { interval } => format!("every:{interval}"),
        SnapshotPolicy::Manual => "manual".into(),
    }
}

fn parse_snapshot_policy(value: &str) -> Result<SnapshotPolicy, EngineError> {
    match value {
        "never" => Ok(SnapshotPolicy::Never),
        "manual" => Ok(SnapshotPolicy::Manual),
        _ => {
            let (kind, raw_interval) =
                value
                    .split_once(':')
                    .ok_or_else(|| EngineError::CorruptedArtifact {
                        artifact: "replay",
                        detail: format!("invalid snapshot policy `{value}`"),
                    })?;
            if kind != "every" {
                return Err(EngineError::CorruptedArtifact {
                    artifact: "replay",
                    detail: format!("invalid snapshot policy `{value}`"),
                });
            }
            Ok(SnapshotPolicy::Every {
                interval: parse_u64(raw_interval, "replay", "snapshot policy interval")?,
            })
        }
    }
}

fn encode_snapshot_capture_reason(reason: &SnapshotCaptureReason) -> String {
    match reason {
        SnapshotCaptureReason::PolicyInterval { interval } => format!("policy_interval:{interval}"),
        SnapshotCaptureReason::Manual => "manual".into(),
    }
}

fn parse_snapshot_capture_reason(value: &str) -> Result<SnapshotCaptureReason, EngineError> {
    match value {
        "manual" => Ok(SnapshotCaptureReason::Manual),
        _ => {
            let (kind, raw_interval) =
                value
                    .split_once(':')
                    .ok_or_else(|| EngineError::CorruptedArtifact {
                        artifact: "replay",
                        detail: format!("invalid snapshot capture reason `{value}`"),
                    })?;
            if kind != "policy_interval" {
                return Err(EngineError::CorruptedArtifact {
                    artifact: "replay",
                    detail: format!("invalid snapshot capture reason `{value}`"),
                });
            }
            Ok(SnapshotCaptureReason::PolicyInterval {
                interval: parse_u64(raw_interval, "replay", "snapshot capture reason interval")?,
            })
        }
    }
}

fn encode_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn parse_bool(value: &str) -> Result<bool, EngineError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(EngineError::CorruptedArtifact {
            artifact: "replay",
            detail: format!("invalid bool `{value}`"),
        }),
    }
}

fn parse_u32(value: &str, artifact: &'static str, field: &str) -> Result<u32, EngineError> {
    value
        .parse()
        .map_err(|error| EngineError::CorruptedArtifact {
            artifact,
            detail: format!("invalid {field} `{value}`: {error}"),
        })
}

fn parse_u64(value: &str, artifact: &'static str, field: &str) -> Result<u64, EngineError> {
    value
        .parse()
        .map_err(|error| EngineError::CorruptedArtifact {
            artifact,
            detail: format!("invalid {field} `{value}`: {error}"),
        })
}

fn parse_usize(value: &str, artifact: &'static str, field: &str) -> Result<usize, EngineError> {
    value
        .parse()
        .map_err(|error| EngineError::CorruptedArtifact {
            artifact,
            detail: format!("invalid {field} `{value}`: {error}"),
        })
}

fn replay_corrupted_canonical(error: CanonicalError) -> EngineError {
    EngineError::CorruptedArtifact {
        artifact: "replay",
        detail: error.to_string(),
    }
}

fn snapshot_corrupted_canonical(error: CanonicalError) -> EngineError {
    EngineError::CorruptedArtifact {
        artifact: "snapshot",
        detail: error.to_string(),
    }
}

fn read_replay_value<'a>(
    reader: &mut CanonicalLineReader<'a>,
    key: &str,
) -> Result<&'a str, EngineError> {
    reader
        .read_value(key, "replay artifact")
        .map_err(replay_corrupted_canonical)
}

fn expect_replay_value(
    reader: &mut CanonicalLineReader<'_>,
    key: &str,
    expected: &str,
) -> Result<(), EngineError> {
    reader
        .expect_value(key, expected, "replay artifact")
        .map_err(replay_corrupted_canonical)
}

fn read_snapshot_value<'a>(
    reader: &mut CanonicalLineReader<'a>,
    key: &str,
) -> Result<&'a str, EngineError> {
    reader
        .read_value(key, "snapshot artifact")
        .map_err(snapshot_corrupted_canonical)
}

fn expect_snapshot_value(
    reader: &mut CanonicalLineReader<'_>,
    key: &str,
    expected: &str,
) -> Result<(), EngineError> {
    reader
        .expect_value(key, expected, "snapshot artifact")
        .map_err(snapshot_corrupted_canonical)
}
