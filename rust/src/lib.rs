#![forbid(unsafe_code)]

pub mod api;
pub mod bindings;
pub mod core;
pub mod engine;
pub mod input;
pub mod phases;
pub mod replay;
pub mod rng;
pub mod scheduler;
pub mod serialization;
pub mod state;

pub use api::{
    CounterCommand, CounterEngine, EngineApi, counter_engine_with_policy, minimal_counter_engine,
};
pub use bindings::EngineBinding;
pub use core::{EngineError, Seed, Tick};
pub use engine::{DeterministicEngine, Engine, SnapshotPolicy, TickResult};
pub use input::{Command, InputFrame};
pub use phases::{Phase, TickContext};
pub use replay::{
    InMemoryReplayLog, PhaseMarker, ReplayDivergence, ReplayLog, ReplayMismatchKind,
    ReplayTickRecord, SnapshotCaptureReason, SnapshotRecord, compare_replay_traces,
};
pub use rng::{Rng, SplitMix64};
pub use scheduler::{FixedScheduler, PhaseDescriptor, PhaseGroup, Scheduler};
pub use serialization::{CounterSnapshotTextSerializer, SerializationError, Serializer};
pub use state::{CounterSnapshot, CounterState, SimulationState};

#[cfg(test)]
mod tests {
    use crate::api::{
        CounterCommand, build_counter_engine, default_counter_scheduler, minimal_counter_engine,
        reordered_counter_scheduler,
    };
    use crate::core::EngineError;
    use crate::engine::{Engine, SnapshotPolicy};
    use crate::input::InputFrame;
    use crate::replay::{ReplayLog, ReplayMismatchKind, compare_replay_traces};
    use crate::scheduler::{PhaseGroup, Scheduler};
    use crate::serialization::{CounterSnapshotTextSerializer, Serializer};
    use crate::state::SimulationState;

    fn sample_frames() -> Vec<InputFrame<CounterCommand>> {
        vec![
            InputFrame::new(
                1,
                CounterCommand {
                    delta: 3,
                    consume_entropy: true,
                },
            ),
            InputFrame::new(
                2,
                CounterCommand {
                    delta: -1,
                    consume_entropy: false,
                },
            ),
            InputFrame::new(
                3,
                CounterCommand {
                    delta: 4,
                    consume_entropy: true,
                },
            ),
            InputFrame::new(
                4,
                CounterCommand {
                    delta: 2,
                    consume_entropy: false,
                },
            ),
        ]
    }

    #[test]
    fn scheduler_plan_is_stable_and_inspectable() {
        let scheduler = default_counter_scheduler();
        let plan = scheduler.phase_plan();

        assert_eq!(
            scheduler.phase_order(),
            vec![
                "reset_finalize_marker",
                "apply_counter_input",
                "simulate_counter",
                "settle_counter",
                "finalize_counter",
            ]
        );
        assert_eq!(
            scheduler.group_members(PhaseGroup::Simulation),
            vec!["simulate_counter"]
        );
        assert_eq!(plan[0].group, PhaseGroup::PreInput);
        assert_eq!(plan[1].group, PhaseGroup::Input);
        assert_eq!(plan[4].group, PhaseGroup::Finalize);
    }

    #[test]
    fn same_seed_and_inputs_produce_identical_replay_checksum_and_snapshots() {
        let frames = sample_frames();
        let mut first = build_counter_engine(
            99,
            default_counter_scheduler(),
            SnapshotPolicy::Every { interval: 2 },
        );
        let mut second = build_counter_engine(
            99,
            default_counter_scheduler(),
            SnapshotPolicy::Every { interval: 2 },
        );

        let mut first_checksums = Vec::new();
        let mut second_checksums = Vec::new();

        for frame in frames.clone() {
            first_checksums.push(
                first
                    .tick(frame.clone())
                    .expect("first deterministic run should succeed")
                    .checksum,
            );
            second_checksums.push(
                second
                    .tick(frame)
                    .expect("second deterministic run should succeed")
                    .checksum,
            );
        }

        assert_eq!(first_checksums, second_checksums);
        assert_eq!(first.state().snapshot(), second.state().snapshot());
        assert_eq!(first.replay_log().records(), second.replay_log().records());
        compare_replay_traces(first.replay_log().records(), second.replay_log().records())
            .expect("matching runs should not diverge");
    }

    #[test]
    fn different_effective_phase_order_changes_result() {
        let frames = sample_frames();
        let mut canonical =
            build_counter_engine(41, default_counter_scheduler(), SnapshotPolicy::Never);
        let mut reordered =
            build_counter_engine(41, reordered_counter_scheduler(), SnapshotPolicy::Never);

        for frame in frames.clone() {
            canonical
                .tick(frame.clone())
                .expect("canonical order should succeed");
            reordered
                .tick(frame)
                .expect("reordered engine should succeed");
        }

        assert_ne!(canonical.state().snapshot(), reordered.state().snapshot());
        assert_ne!(
            canonical.replay_log().records(),
            reordered.replay_log().records()
        );
    }

    #[test]
    fn skipped_or_unordered_tick_inputs_fail_fast() {
        let mut engine = minimal_counter_engine(7);
        engine
            .tick(InputFrame::new(
                1,
                CounterCommand {
                    delta: 1,
                    consume_entropy: false,
                },
            ))
            .expect("tick 1 should succeed");

        let skipped = engine.tick(InputFrame::new(
            3,
            CounterCommand {
                delta: 1,
                consume_entropy: false,
            },
        ));
        assert_eq!(
            skipped,
            Err(EngineError::UnexpectedInputTick {
                expected: 2,
                got: 3
            })
        );

        let backwards = engine.tick(InputFrame::new(
            1,
            CounterCommand {
                delta: 1,
                consume_entropy: false,
            },
        ));
        assert_eq!(
            backwards,
            Err(EngineError::UnexpectedInputTick {
                expected: 2,
                got: 1
            })
        );
    }

    #[test]
    fn snapshot_policy_every_n_ticks_captures_on_schedule() {
        let mut engine = build_counter_engine(
            17,
            default_counter_scheduler(),
            SnapshotPolicy::Every { interval: 2 },
        );

        let mut captured_ticks = Vec::new();
        for frame in sample_frames() {
            let result = engine
                .tick(frame)
                .expect("deterministic tick should succeed");
            if result.snapshot.is_some() {
                captured_ticks.push(result.tick);
            }
        }

        let snapshot_ticks = engine
            .replay_log()
            .records()
            .iter()
            .filter_map(|record| record.snapshot.as_ref().map(|snapshot| snapshot.tick))
            .collect::<Vec<_>>();

        assert_eq!(captured_ticks, vec![2, 4]);
        assert_eq!(snapshot_ticks, vec![2, 4]);
    }

    #[test]
    fn replay_divergence_checker_reports_first_mismatch() {
        let frames = sample_frames();
        let mut expected =
            build_counter_engine(77, default_counter_scheduler(), SnapshotPolicy::Never);
        let mut actual =
            build_counter_engine(77, default_counter_scheduler(), SnapshotPolicy::Never);

        for frame in frames.iter().take(2).cloned() {
            expected
                .tick(frame.clone())
                .expect("expected run should succeed");
            actual.tick(frame).expect("actual run should succeed");
        }

        expected
            .tick(InputFrame::new(
                3,
                CounterCommand {
                    delta: 4,
                    consume_entropy: true,
                },
            ))
            .expect("expected third tick should succeed");
        actual
            .tick(InputFrame::new(
                3,
                CounterCommand {
                    delta: 40,
                    consume_entropy: true,
                },
            ))
            .expect("actual third tick should succeed");

        let divergence = compare_replay_traces(
            expected.replay_log().records(),
            actual.replay_log().records(),
        )
        .expect_err("replay traces should diverge");

        assert_eq!(divergence.record_index, 2);
        assert_eq!(divergence.tick, Some(3));
        assert!(matches!(divergence.kind, ReplayMismatchKind::Input { .. }));
    }

    #[test]
    fn restoring_a_snapshot_and_continuing_matches_full_run() {
        let frames = sample_frames();
        let mut full_run = build_counter_engine(
            123,
            default_counter_scheduler(),
            SnapshotPolicy::Every { interval: 2 },
        );

        for frame in frames.clone() {
            full_run.tick(frame).expect("full run should succeed");
        }

        let snapshot = full_run.replay_log().records()[1]
            .snapshot
            .as_ref()
            .expect("tick 2 snapshot should exist")
            .payload
            .clone();
        let expected_checksum = full_run
            .replay_log()
            .records()
            .last()
            .expect("full replay should have records")
            .checksum;

        let mut resumed = build_counter_engine(
            123,
            default_counter_scheduler(),
            SnapshotPolicy::Every { interval: 2 },
        );
        resumed.restore_snapshot(snapshot);

        for frame in sample_frames().into_iter().skip(2) {
            resumed.tick(frame).expect("resumed run should succeed");
        }

        let resumed_checksum = resumed
            .replay_log()
            .records()
            .last()
            .expect("resumed replay should have records")
            .checksum;

        assert_eq!(full_run.state().snapshot(), resumed.state().snapshot());
        assert_eq!(expected_checksum, resumed_checksum);
    }

    #[test]
    fn golden_style_replay_trace_comparison_passes_for_matching_trace() {
        let mut engine = build_counter_engine(
            55,
            default_counter_scheduler(),
            SnapshotPolicy::Every { interval: 2 },
        );

        for frame in sample_frames() {
            engine.tick(frame).expect("tick should succeed");
        }

        let golden = engine.replay_log().records().to_vec();
        compare_replay_traces(&golden, engine.replay_log().records())
            .expect("golden replay should match live replay");
    }

    #[test]
    fn snapshot_serialization_roundtrip_remains_deterministic() {
        let mut engine = build_counter_engine(
            88,
            default_counter_scheduler(),
            SnapshotPolicy::Every { interval: 1 },
        );
        engine
            .tick(InputFrame::new(
                1,
                CounterCommand {
                    delta: 5,
                    consume_entropy: true,
                },
            ))
            .expect("tick should succeed");

        let serializer = CounterSnapshotTextSerializer;
        let encoded = engine
            .serialize_snapshot_with(&serializer)
            .expect("snapshot serialization should succeed");
        let decoded = serializer
            .decode(&encoded)
            .expect("snapshot decoding should succeed");

        assert_eq!(decoded, engine.state().snapshot());
    }
}
