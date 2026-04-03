use std::fmt;

use crate::core::{EngineError, Seed, Tick};
use crate::input::{Command, InputFrame};
use crate::scheduler::{PhaseDescriptor, PhaseGroup};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhaseMarker {
    pub ordinal: usize,
    pub group: PhaseGroup,
    pub name: String,
}

impl From<PhaseDescriptor> for PhaseMarker {
    fn from(value: PhaseDescriptor) -> Self {
        Self {
            ordinal: value.ordinal,
            group: value.group,
            name: value.name.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotCaptureReason {
    PolicyInterval { interval: Tick },
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapshotMetadata {
    pub payload_schema_version: u32,
    pub source_tick: Tick,
    pub capture_checksum: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotRecord<Snapshot>
where
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub metadata: SnapshotMetadata,
    pub reason: SnapshotCaptureReason,
    pub payload: Snapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayTickRecord<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub tick: Tick,
    pub input: InputFrame<C>,
    pub tick_seed: Seed,
    pub phase_markers: Vec<PhaseMarker>,
    pub checksum: u64,
    pub snapshot: Option<SnapshotRecord<Snapshot>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingReplayTickRecord<C>
where
    C: Command,
{
    tick: Tick,
    input: InputFrame<C>,
    tick_seed: Seed,
    phase_markers: Vec<PhaseMarker>,
}

pub trait ReplayLog<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    fn begin_tick(&mut self, frame: &InputFrame<C>, tick_seed: Seed) -> Result<(), EngineError>;
    fn record_phase(&mut self, marker: PhaseMarker) -> Result<(), EngineError>;
    fn complete_tick(
        &mut self,
        checksum: u64,
        snapshot: Option<SnapshotRecord<Snapshot>>,
    ) -> Result<(), EngineError>;
    fn records(&self) -> &[ReplayTickRecord<C, Snapshot>];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InMemoryReplayLog<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    records: Vec<ReplayTickRecord<C, Snapshot>>,
    pending: Option<PendingReplayTickRecord<C>>,
}

impl<C, Snapshot> Default for InMemoryReplayLog<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    fn default() -> Self {
        Self {
            records: Vec::new(),
            pending: None,
        }
    }
}

impl<C, Snapshot> ReplayLog<C, Snapshot> for InMemoryReplayLog<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    fn begin_tick(&mut self, frame: &InputFrame<C>, tick_seed: Seed) -> Result<(), EngineError> {
        if self.pending.is_some() {
            return Err(EngineError::ReplayLifecycle {
                detail: "attempted to begin a tick while another tick is pending".into(),
            });
        }

        self.pending = Some(PendingReplayTickRecord {
            tick: frame.tick,
            input: frame.clone(),
            tick_seed,
            phase_markers: Vec::new(),
        });

        Ok(())
    }

    fn record_phase(&mut self, marker: PhaseMarker) -> Result<(), EngineError> {
        let pending = self
            .pending
            .as_mut()
            .ok_or_else(|| EngineError::ReplayLifecycle {
                detail: format!(
                    "attempted to record phase `{}` without a pending replay tick",
                    marker.name
                ),
            })?;

        pending.phase_markers.push(marker);
        Ok(())
    }

    fn complete_tick(
        &mut self,
        checksum: u64,
        snapshot: Option<SnapshotRecord<Snapshot>>,
    ) -> Result<(), EngineError> {
        let pending = self
            .pending
            .take()
            .ok_or_else(|| EngineError::ReplayLifecycle {
                detail: "attempted to finalize a replay tick without begin_tick".into(),
            })?;

        self.records.push(ReplayTickRecord {
            tick: pending.tick,
            input: pending.input,
            tick_seed: pending.tick_seed,
            phase_markers: pending.phase_markers,
            checksum,
            snapshot,
        });

        Ok(())
    }

    fn records(&self) -> &[ReplayTickRecord<C, Snapshot>] {
        &self.records
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayTickSummary {
    pub tick: Tick,
    pub input_tick: Tick,
    pub tick_seed: Seed,
    pub phase_markers: Vec<PhaseMarker>,
    pub checksum: u64,
    pub snapshot_present: bool,
    pub snapshot_source_tick: Option<Tick>,
    pub snapshot_capture_checksum: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayInspectionView {
    pub record_count: usize,
    pub final_tick: Tick,
    pub tick_summaries: Vec<ReplayTickSummary>,
}

pub fn inspect_replay_trace<C, Snapshot>(
    records: &[ReplayTickRecord<C, Snapshot>],
) -> ReplayInspectionView
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    let tick_summaries = records
        .iter()
        .map(|record| ReplayTickSummary {
            tick: record.tick,
            input_tick: record.input.tick,
            tick_seed: record.tick_seed,
            phase_markers: record.phase_markers.clone(),
            checksum: record.checksum,
            snapshot_present: record.snapshot.is_some(),
            snapshot_source_tick: record
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.metadata.source_tick),
            snapshot_capture_checksum: record
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.metadata.capture_checksum),
        })
        .collect::<Vec<_>>();

    ReplayInspectionView {
        record_count: records.len(),
        final_tick: records.last().map(|record| record.tick).unwrap_or(0),
        tick_summaries,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayMismatchKind<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    TickCount {
        expected: usize,
        actual: usize,
    },
    Tick {
        expected: Tick,
        actual: Tick,
    },
    Input {
        expected: InputFrame<C>,
        actual: InputFrame<C>,
    },
    TickSeed {
        expected: Seed,
        actual: Seed,
    },
    PhaseMarkers {
        expected: Vec<PhaseMarker>,
        actual: Vec<PhaseMarker>,
    },
    Checksum {
        expected: u64,
        actual: u64,
    },
    SnapshotPresence {
        expected: bool,
        actual: bool,
    },
    SnapshotReason {
        expected: SnapshotCaptureReason,
        actual: SnapshotCaptureReason,
    },
    SnapshotMetadata {
        expected: SnapshotMetadata,
        actual: SnapshotMetadata,
    },
    SnapshotPayloadDigest {
        expected: u64,
        actual: u64,
    },
    SnapshotPayload {
        expected: SnapshotRecord<Snapshot>,
        actual: SnapshotRecord<Snapshot>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayDivergence<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    pub record_index: usize,
    pub tick: Option<Tick>,
    pub kind: ReplayMismatchKind<C, Snapshot>,
}

impl<C, Snapshot> fmt::Display for ReplayDivergence<C, Snapshot>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ReplayMismatchKind::TickCount { expected, actual } => write!(
                f,
                "tick count mismatch at index {}: expected {}, got {}",
                self.record_index, expected, actual
            ),
            ReplayMismatchKind::Tick { expected, actual } => {
                write!(f, "tick mismatch: expected {expected}, got {actual}")
            }
            ReplayMismatchKind::Input { expected, actual } => {
                write!(
                    f,
                    "input mismatch at index {}: expected {:?}, got {:?}",
                    self.record_index, expected, actual
                )
            }
            ReplayMismatchKind::TickSeed { expected, actual } => {
                write!(
                    f,
                    "tick seed mismatch at index {}: expected {expected}, got {actual}",
                    self.record_index
                )
            }
            ReplayMismatchKind::PhaseMarkers { expected, actual } => {
                write!(
                    f,
                    "phase marker mismatch at index {}: expected {:?}, got {:?}",
                    self.record_index, expected, actual
                )
            }
            ReplayMismatchKind::Checksum { expected, actual } => {
                write!(
                    f,
                    "checksum mismatch at index {}: expected {expected}, got {actual}",
                    self.record_index
                )
            }
            ReplayMismatchKind::SnapshotPresence { expected, actual } => write!(
                f,
                "snapshot presence mismatch at index {}: expected {expected}, got {actual}",
                self.record_index
            ),
            ReplayMismatchKind::SnapshotReason { expected, actual } => write!(
                f,
                "snapshot reason mismatch at index {}: expected {:?}, got {:?}",
                self.record_index, expected, actual
            ),
            ReplayMismatchKind::SnapshotMetadata { expected, actual } => write!(
                f,
                "snapshot metadata mismatch at index {}: expected {:?}, got {:?}",
                self.record_index, expected, actual
            ),
            ReplayMismatchKind::SnapshotPayloadDigest { expected, actual } => write!(
                f,
                "snapshot payload digest mismatch at index {}: expected {}, got {}",
                self.record_index, expected, actual
            ),
            ReplayMismatchKind::SnapshotPayload { expected, actual } => write!(
                f,
                "snapshot payload mismatch at index {}: expected {:?}, got {:?}",
                self.record_index, expected, actual
            ),
        }
    }
}

impl<C, Snapshot> From<ReplayDivergence<C, Snapshot>> for EngineError
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    fn from(value: ReplayDivergence<C, Snapshot>) -> Self {
        EngineError::ReplayMismatch {
            tick: value.tick,
            detail: value.to_string(),
        }
    }
}

pub fn compare_replay_traces<C, Snapshot>(
    expected: &[ReplayTickRecord<C, Snapshot>],
    actual: &[ReplayTickRecord<C, Snapshot>],
) -> Result<(), ReplayDivergence<C, Snapshot>>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
{
    compare_replay_traces_with_snapshot_digest(
        expected,
        actual,
        |_snapshot| -> Result<u64, fmt::Error> { Err(fmt::Error) },
    )
}

pub fn compare_replay_traces_with_snapshot_digest<C, Snapshot, D, E>(
    expected: &[ReplayTickRecord<C, Snapshot>],
    actual: &[ReplayTickRecord<C, Snapshot>],
    snapshot_digest: D,
) -> Result<(), ReplayDivergence<C, Snapshot>>
where
    C: Command,
    Snapshot: Clone + Eq + fmt::Debug,
    D: Fn(&Snapshot) -> Result<u64, E>,
{
    let shared_len = expected.len().min(actual.len());

    for index in 0..shared_len {
        let expected_record = &expected[index];
        let actual_record = &actual[index];
        let tick = Some(expected_record.tick);

        if expected_record.tick != actual_record.tick {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::Tick {
                    expected: expected_record.tick,
                    actual: actual_record.tick,
                },
            });
        }

        if expected_record.input != actual_record.input {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::Input {
                    expected: expected_record.input.clone(),
                    actual: actual_record.input.clone(),
                },
            });
        }

        if expected_record.tick_seed != actual_record.tick_seed {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::TickSeed {
                    expected: expected_record.tick_seed,
                    actual: actual_record.tick_seed,
                },
            });
        }

        if expected_record.phase_markers != actual_record.phase_markers {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::PhaseMarkers {
                    expected: expected_record.phase_markers.clone(),
                    actual: actual_record.phase_markers.clone(),
                },
            });
        }

        if expected_record.checksum != actual_record.checksum {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::Checksum {
                    expected: expected_record.checksum,
                    actual: actual_record.checksum,
                },
            });
        }

        if expected_record.snapshot.is_some() != actual_record.snapshot.is_some() {
            return Err(ReplayDivergence {
                record_index: index,
                tick,
                kind: ReplayMismatchKind::SnapshotPresence {
                    expected: expected_record.snapshot.is_some(),
                    actual: actual_record.snapshot.is_some(),
                },
            });
        }

        if let (Some(expected_snapshot), Some(actual_snapshot)) =
            (&expected_record.snapshot, &actual_record.snapshot)
        {
            if expected_snapshot.reason != actual_snapshot.reason {
                return Err(ReplayDivergence {
                    record_index: index,
                    tick,
                    kind: ReplayMismatchKind::SnapshotReason {
                        expected: expected_snapshot.reason.clone(),
                        actual: actual_snapshot.reason.clone(),
                    },
                });
            }

            if expected_snapshot.metadata != actual_snapshot.metadata {
                return Err(ReplayDivergence {
                    record_index: index,
                    tick,
                    kind: ReplayMismatchKind::SnapshotMetadata {
                        expected: expected_snapshot.metadata,
                        actual: actual_snapshot.metadata,
                    },
                });
            }

            if expected_snapshot.payload != actual_snapshot.payload {
                match (
                    snapshot_digest(&expected_snapshot.payload),
                    snapshot_digest(&actual_snapshot.payload),
                ) {
                    (Ok(expected_digest), Ok(actual_digest)) => {
                        return Err(ReplayDivergence {
                            record_index: index,
                            tick,
                            kind: ReplayMismatchKind::SnapshotPayloadDigest {
                                expected: expected_digest,
                                actual: actual_digest,
                            },
                        });
                    }
                    _ => {
                        return Err(ReplayDivergence {
                            record_index: index,
                            tick,
                            kind: ReplayMismatchKind::SnapshotPayload {
                                expected: expected_snapshot.clone(),
                                actual: actual_snapshot.clone(),
                            },
                        });
                    }
                }
            }
        }
    }

    if expected.len() != actual.len() {
        let tick = if let Some(record) = expected.get(shared_len) {
            Some(record.tick)
        } else {
            actual.get(shared_len).map(|record| record.tick)
        };

        return Err(ReplayDivergence {
            record_index: shared_len,
            tick,
            kind: ReplayMismatchKind::TickCount {
                expected: expected.len(),
                actual: actual.len(),
            },
        });
    }

    Ok(())
}
