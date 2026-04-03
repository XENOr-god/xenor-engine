#![forbid(unsafe_code)]

pub mod api;
pub mod bindings;
mod canonical;
pub mod core;
pub mod engine;
pub mod fixture;
pub mod input;
pub mod parity;
pub mod persistence;
pub mod phases;
pub mod replay;
pub mod rng;
pub mod scheduler;
pub mod serialization;
pub mod state;

pub use api::{
    COUNTER_ENGINE_FAMILY, CounterCommand, CounterEngine, CounterGoldenFixture,
    CounterGoldenFixtureCodec, CounterGoldenFixtureResult, CounterParitySummary,
    CounterRecordedReplay, CounterReplayArtifact, CounterReplayArtifactCodec, CounterReplayResult,
    CounterSnapshotArtifact, EngineApi, counter_engine_with_policy, counter_golden_fixture_codec,
    counter_parity_summary, counter_replay_artifact_codec, counter_replay_summary,
    counter_snapshot_artifact_at_tick, counter_snapshot_artifact_codec,
    export_counter_golden_fixture, export_counter_replay_artifact,
    export_counter_snapshot_artifact, generate_counter_golden_fixture,
    import_counter_golden_fixture, import_counter_replay_artifact,
    import_counter_snapshot_artifact, inspect_counter_replay, minimal_counter_engine,
    record_counter_replay, resume_counter_replay_from_snapshot, verify_counter_golden_fixture,
    verify_counter_replay,
};
pub use bindings::EngineBinding;
pub use core::{EngineError, Seed, Tick};
pub use engine::{DeterministicEngine, Engine, ReplayableEngine, SnapshotPolicy, TickResult};
pub use fixture::{
    GOLDEN_FIXTURE_SCHEMA_VERSION, GoldenFixture, GoldenFixtureMetadata, GoldenFixtureResult,
    GoldenFixtureSerializer, GoldenFixtureSummary,
};
pub use input::{Command, InputFrame};
pub use parity::{
    ParityArtifactSummary, ParityComparison, ParityMismatch, compare_parity_summaries,
};
pub use persistence::{
    ArtifactSummary, REPLAY_ARTIFACT_SCHEMA_VERSION, RecordedReplay, ReplayArtifact,
    ReplayArtifactMetadata, ReplayArtifactSerializer, ReplayExecutionMode, ReplayExecutionResult,
    SNAPSHOT_ARTIFACT_SCHEMA_VERSION, SnapshotArtifact, SnapshotArtifactMetadata,
    SnapshotArtifactSerializer, execute_replay_from_snapshot, execute_replay_verify, record_replay,
    validate_snapshot_artifact,
};
pub use phases::{Phase, TickContext};
pub use replay::{
    InMemoryReplayLog, PhaseMarker, ReplayDivergence, ReplayInspectionView, ReplayLog,
    ReplayMismatchKind, ReplayTickRecord, ReplayTickSummary, SnapshotCaptureReason,
    SnapshotMetadata, SnapshotRecord, compare_replay_traces,
    compare_replay_traces_with_snapshot_digest, inspect_replay_trace,
};
pub use rng::{Rng, SplitMix64};
pub use scheduler::{FixedScheduler, PhaseDescriptor, PhaseGroup, Scheduler};
pub use serialization::{
    CounterCommandTextSerializer, CounterSnapshotTextSerializer, SerializationError, Serializer,
};
pub use state::{CounterSnapshot, CounterState, SimulationState};

#[cfg(test)]
mod tests {
    use crate::api::{
        CounterCommand, build_counter_engine, counter_golden_fixture_codec, counter_parity_summary,
        counter_replay_artifact_codec, counter_replay_summary, default_counter_scheduler,
        export_counter_golden_fixture, export_counter_replay_artifact,
        export_counter_snapshot_artifact, generate_counter_golden_fixture,
        import_counter_golden_fixture, import_counter_replay_artifact,
        import_counter_snapshot_artifact, inspect_counter_replay, minimal_counter_engine,
        record_counter_replay, reordered_counter_scheduler, resume_counter_replay_from_snapshot,
        verify_counter_golden_fixture, verify_counter_replay,
    };
    use crate::core::EngineError;
    use crate::engine::{Engine, SnapshotPolicy};
    use crate::input::InputFrame;
    use crate::parity::{ParityArtifactSummary, ParityMismatch, compare_parity_summaries};
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

    fn remove_line(text: &str, prefix: &str) -> String {
        let kept = text
            .lines()
            .filter(|line| !line.starts_with(prefix))
            .collect::<Vec<_>>();
        format!("{}\n", kept.join("\n"))
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
            .filter_map(|record| {
                record
                    .snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.metadata.source_tick)
            })
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
    fn replay_divergence_reports_tick_count_mismatch() {
        let recorded = record_counter_replay(80, SnapshotPolicy::Never, &sample_frames())
            .expect("record mode should succeed");
        let shorter = &recorded.artifact.records[..2];
        let longer = &recorded.artifact.records[..3];

        let divergence =
            compare_replay_traces(shorter, longer).expect_err("tick count mismatch should fail");

        assert_eq!(divergence.record_index, 2);
        assert_eq!(divergence.tick, Some(3));
        assert!(matches!(
            divergence.kind,
            ReplayMismatchKind::TickCount {
                expected: 2,
                actual: 3
            }
        ));
    }

    #[test]
    fn replay_inspection_view_is_deterministic() {
        let recorded =
            record_counter_replay(81, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");

        let first = inspect_counter_replay(&recorded.artifact);
        let second = inspect_counter_replay(&recorded.artifact);

        assert_eq!(first, second);
        assert_eq!(first.record_count, 4);
        assert_eq!(first.final_tick, 4);
        assert_eq!(
            first.tick_summaries[1]
                .phase_markers
                .iter()
                .map(|marker| marker.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "reset_finalize_marker",
                "apply_counter_input",
                "simulate_counter",
                "settle_counter",
                "finalize_counter",
            ]
        );
        assert!(first.tick_summaries[1].snapshot_present);
    }

    #[test]
    fn replay_artifact_roundtrip_serialize_deserialize() {
        let recorded =
            record_counter_replay(90, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let bytes = export_counter_replay_artifact(&recorded.artifact)
            .expect("replay artifact should serialize");
        let decoded =
            import_counter_replay_artifact(&bytes).expect("replay artifact should deserialize");

        assert_eq!(recorded.artifact, decoded);
    }

    #[test]
    fn snapshot_artifact_roundtrip_serialize_deserialize() {
        let recorded =
            record_counter_replay(91, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let snapshot = crate::api::counter_snapshot_artifact_at_tick(&recorded.artifact, 2)
            .expect("tick 2 snapshot should exist");
        let bytes = export_counter_snapshot_artifact(&snapshot)
            .expect("snapshot artifact should serialize");
        let decoded =
            import_counter_snapshot_artifact(&bytes).expect("snapshot artifact should deserialize");

        assert_eq!(snapshot, decoded);
    }

    #[test]
    fn replay_export_import_export_is_canonical_and_identical() {
        let recorded =
            record_counter_replay(92, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let first = export_counter_replay_artifact(&recorded.artifact)
            .expect("replay artifact should serialize");
        let imported =
            import_counter_replay_artifact(&first).expect("replay artifact should deserialize");
        let second = export_counter_replay_artifact(&imported)
            .expect("re-exported replay artifact should serialize");

        assert_eq!(first, second);
    }

    #[test]
    fn snapshot_export_import_export_is_canonical_and_identical() {
        let recorded =
            record_counter_replay(93, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let snapshot = crate::api::counter_snapshot_artifact_at_tick(&recorded.artifact, 2)
            .expect("tick 2 snapshot should exist");
        let first = export_counter_snapshot_artifact(&snapshot)
            .expect("snapshot artifact should serialize");
        let imported =
            import_counter_snapshot_artifact(&first).expect("snapshot artifact should deserialize");
        let second = export_counter_snapshot_artifact(&imported)
            .expect("re-exported snapshot artifact should serialize");

        assert_eq!(first, second);
    }

    #[test]
    fn golden_fixture_export_import_export_is_canonical_and_identical() {
        let fixture = generate_counter_golden_fixture(
            94,
            SnapshotPolicy::Every { interval: 2 },
            &sample_frames(),
        )
        .expect("fixture generation should succeed");
        let first = export_counter_golden_fixture(&fixture).expect("fixture export should succeed");
        let imported =
            import_counter_golden_fixture(&first).expect("fixture import should succeed");
        let second =
            export_counter_golden_fixture(&imported).expect("fixture re-export should succeed");

        assert_eq!(first, second);
    }

    #[test]
    fn canonical_digest_is_stable_for_same_replay_artifact() {
        let recorded =
            record_counter_replay(95, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let codec = counter_replay_artifact_codec();
        let imported = import_counter_replay_artifact(
            &export_counter_replay_artifact(&recorded.artifact)
                .expect("replay artifact should serialize"),
        )
        .expect("replay artifact should deserialize");

        assert_eq!(
            codec
                .digest(&recorded.artifact)
                .expect("digest should be available"),
            codec.digest(&imported).expect("digest should be available"),
        );
    }

    #[test]
    fn full_run_matches_resume_from_snapshot() {
        let recorded =
            record_counter_replay(123, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let snapshot = crate::api::counter_snapshot_artifact_at_tick(&recorded.artifact, 2)
            .expect("tick 2 snapshot should exist");

        let full = verify_counter_replay(&recorded.artifact)
            .expect("full replay verification should succeed");
        let resumed = resume_counter_replay_from_snapshot(&snapshot, &recorded.artifact)
            .expect("resume from snapshot should succeed");

        assert_eq!(full.final_tick, resumed.final_tick);
        assert_eq!(full.final_checksum, resumed.final_checksum);
        assert_eq!(full.summary, resumed.summary);
    }

    #[test]
    fn unsupported_replay_version_fails_fast() {
        let recorded = record_counter_replay(44, SnapshotPolicy::Never, &sample_frames())
            .expect("record mode should succeed");
        let bytes = export_counter_replay_artifact(&recorded.artifact)
            .expect("replay artifact should serialize");
        let text = String::from_utf8(bytes).expect("artifact bytes should be utf8");
        let tampered = text.replace("artifact_schema_version=1", "artifact_schema_version=9");

        let error = import_counter_replay_artifact(tampered.as_bytes())
            .expect_err("unsupported replay version should fail");

        assert_eq!(
            error,
            EngineError::UnsupportedSchemaVersion {
                artifact: "replay artifact",
                expected: 1,
                got: 9
            }
        );
    }

    #[test]
    fn invalid_snapshot_payload_schema_version_fails_fast() {
        let recorded =
            record_counter_replay(45, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let snapshot = crate::api::counter_snapshot_artifact_at_tick(&recorded.artifact, 2)
            .expect("tick 2 snapshot should exist");
        let bytes = export_counter_snapshot_artifact(&snapshot)
            .expect("snapshot artifact should serialize");
        let text = String::from_utf8(bytes).expect("artifact bytes should be utf8");
        let tampered = text.replace(
            "snapshot_payload_schema_version=1",
            "snapshot_payload_schema_version=9",
        );

        let error = import_counter_snapshot_artifact(tampered.as_bytes())
            .expect_err("unsupported snapshot version should fail");

        assert_eq!(
            error,
            EngineError::UnsupportedSchemaVersion {
                artifact: "snapshot payload",
                expected: 1,
                got: 9
            }
        );
    }

    #[test]
    fn invalid_command_payload_schema_version_fails_fast() {
        let recorded = record_counter_replay(46, SnapshotPolicy::Never, &sample_frames())
            .expect("record mode should succeed");
        let bytes = export_counter_replay_artifact(&recorded.artifact)
            .expect("replay artifact should serialize");
        let text = String::from_utf8(bytes).expect("artifact bytes should be utf8");
        let tampered = text.replace(
            "command_payload_schema_version=1",
            "command_payload_schema_version=9",
        );

        let error = import_counter_replay_artifact(tampered.as_bytes())
            .expect_err("unsupported command payload version should fail");

        assert_eq!(
            error,
            EngineError::UnsupportedSchemaVersion {
                artifact: "replay command payload",
                expected: 1,
                got: 9
            }
        );
    }

    #[test]
    fn malformed_duplicate_replay_section_fails_decode() {
        let recorded = record_counter_replay(47, SnapshotPolicy::Never, &sample_frames())
            .expect("record mode should succeed");
        let bytes = export_counter_replay_artifact(&recorded.artifact)
            .expect("replay artifact should serialize");
        let text = String::from_utf8(bytes).expect("artifact bytes should be utf8");
        let tampered = format!("artifact=replay\n{text}");

        let error = import_counter_replay_artifact(tampered.as_bytes())
            .expect_err("duplicate artifact header should fail");

        assert!(matches!(
            error,
            EngineError::CorruptedArtifact {
                artifact: "replay",
                ..
            }
        ));
    }

    #[test]
    fn missing_required_snapshot_section_fails_decode() {
        let recorded =
            record_counter_replay(48, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let snapshot = crate::api::counter_snapshot_artifact_at_tick(&recorded.artifact, 2)
            .expect("tick 2 snapshot should exist");
        let bytes = export_counter_snapshot_artifact(&snapshot)
            .expect("snapshot artifact should serialize");
        let text = String::from_utf8(bytes).expect("artifact bytes should be utf8");
        let tampered = remove_line(&text, "payload_hex=");

        let error = import_counter_snapshot_artifact(tampered.as_bytes())
            .expect_err("missing payload_hex should fail");

        assert!(matches!(
            error,
            EngineError::CorruptedArtifact {
                artifact: "snapshot",
                ..
            }
        ));
    }

    #[test]
    fn corrupted_replay_artifact_fails_fast() {
        let recorded = record_counter_replay(49, SnapshotPolicy::Never, &sample_frames())
            .expect("record mode should succeed");
        let bytes = export_counter_replay_artifact(&recorded.artifact)
            .expect("replay artifact should serialize");
        let text = String::from_utf8(bytes).expect("artifact bytes should be utf8");
        let tampered = text.replacen("record.0.command_hex=", "record.0.command_hex=abc", 1);

        let error = import_counter_replay_artifact(tampered.as_bytes())
            .expect_err("corrupted replay artifact should fail");

        assert!(matches!(
            error,
            EngineError::CorruptedArtifact {
                artifact: "replay",
                ..
            }
        ));
    }

    #[test]
    fn seed_mismatch_fails_on_resume() {
        let recorded =
            record_counter_replay(50, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let mut snapshot = crate::api::counter_snapshot_artifact_at_tick(&recorded.artifact, 2)
            .expect("tick 2 snapshot should exist");
        snapshot.metadata.base_seed += 1;

        let error = resume_counter_replay_from_snapshot(&snapshot, &recorded.artifact)
            .expect_err("seed mismatch should fail");

        assert_eq!(
            error,
            EngineError::SeedMismatch {
                expected: 50,
                got: 51
            }
        );
    }

    #[test]
    fn snapshot_tick_mismatch_with_continuation_fails_fast() {
        let recorded =
            record_counter_replay(51, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let snapshot = crate::api::counter_snapshot_artifact_at_tick(&recorded.artifact, 2)
            .expect("tick 2 snapshot should exist");
        let mut mismatched = recorded.artifact.clone();
        mismatched.records.remove(2);
        mismatched.metadata.record_count = mismatched.records.len();

        let error = resume_counter_replay_from_snapshot(&snapshot, &mismatched)
            .expect_err("resume tick mismatch should fail");

        assert_eq!(
            error,
            EngineError::ResumeTickMismatch {
                expected: 3,
                got: 4
            }
        );
    }

    #[test]
    fn artifact_summary_is_stable_for_golden_fixture_use() {
        let recorded =
            record_counter_replay(52, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let summary =
            counter_replay_summary(&recorded.artifact).expect("summary generation should succeed");

        assert_eq!(
            summary.to_text(),
            "engine_family=xenor-engine-rust/counter\nbase_seed=52\nfinal_tick=4\nfinal_checksum=9273508064903698236\nreplay_artifact_schema_version=1\nsnapshot_artifact_schema_version=1\ncommand_payload_schema_version=1\nsnapshot_payload_schema_version=1\nreplay_digest=4362292177231439159\nsnapshot_digest=15161838349500027775\n"
        );
    }

    #[test]
    fn imported_replay_remains_identical_to_original_trace() {
        let recorded =
            record_counter_replay(53, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let bytes = export_counter_replay_artifact(&recorded.artifact)
            .expect("replay artifact should serialize");
        let imported =
            import_counter_replay_artifact(&bytes).expect("replay artifact should deserialize");

        compare_replay_traces(
            recorded.artifact.records.as_slice(),
            imported.records.as_slice(),
        )
        .expect("imported replay should match original trace");
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

    #[test]
    fn generated_golden_fixture_verifies_against_rerun() {
        let fixture = generate_counter_golden_fixture(
            54,
            SnapshotPolicy::Every { interval: 2 },
            &sample_frames(),
        )
        .expect("fixture generation should succeed");
        let result =
            verify_counter_golden_fixture(&fixture).expect("fixture verification should run");

        assert!(result.passed());
        assert!(result.replay_mismatch.is_none());
        assert!(result.snapshot_mismatch.is_none());
    }

    #[test]
    fn golden_fixture_with_tampered_replay_artifact_fails() {
        let mut fixture = generate_counter_golden_fixture(
            55,
            SnapshotPolicy::Every { interval: 2 },
            &sample_frames(),
        )
        .expect("fixture generation should succeed");
        fixture.replay_artifact.records[2].input.command.delta += 1;

        let result =
            verify_counter_golden_fixture(&fixture).expect("fixture verification should run");

        assert!(!result.passed());
        assert!(
            result
                .replay_mismatch
                .as_deref()
                .expect("replay mismatch should be present")
                .contains("checksum mismatch")
        );
    }

    #[test]
    fn golden_fixture_with_tampered_summary_fails() {
        let mut fixture = generate_counter_golden_fixture(
            56,
            SnapshotPolicy::Every { interval: 2 },
            &sample_frames(),
        )
        .expect("fixture generation should succeed");
        fixture.summary.final_checksum ^= 1;

        let result =
            verify_counter_golden_fixture(&fixture).expect("fixture verification should run");

        assert!(!result.passed());
        assert!(matches!(
            result.comparison.first_mismatch(),
            Some(ParityMismatch::FinalChecksum { .. })
        ));
    }

    #[test]
    fn golden_fixture_with_seed_mismatch_fails() {
        let mut fixture = generate_counter_golden_fixture(
            57,
            SnapshotPolicy::Every { interval: 2 },
            &sample_frames(),
        )
        .expect("fixture generation should succeed");
        fixture.summary.base_seed += 1;

        let result =
            verify_counter_golden_fixture(&fixture).expect("fixture verification should run");

        assert!(!result.passed());
        assert!(matches!(
            result.comparison.first_mismatch(),
            Some(ParityMismatch::BaseSeed { .. })
        ));
    }

    #[test]
    fn golden_fixture_with_snapshot_mismatch_fails() {
        let mut fixture = generate_counter_golden_fixture(
            58,
            SnapshotPolicy::Every { interval: 2 },
            &sample_frames(),
        )
        .expect("fixture generation should succeed");
        fixture
            .snapshot_artifact
            .as_mut()
            .expect("fixture should carry a snapshot artifact")
            .payload
            .value += 1;

        let result =
            verify_counter_golden_fixture(&fixture).expect("fixture verification should run");

        assert!(!result.passed());
        assert!(
            result
                .snapshot_mismatch
                .as_deref()
                .expect("snapshot mismatch should be present")
                .contains("snapshot payload digest mismatch")
        );
    }

    #[test]
    fn golden_fixture_verification_report_shows_expected_mismatch() {
        let mut fixture = generate_counter_golden_fixture(
            59,
            SnapshotPolicy::Every { interval: 2 },
            &sample_frames(),
        )
        .expect("fixture generation should succeed");
        fixture.summary.final_checksum ^= 1;

        let result =
            verify_counter_golden_fixture(&fixture).expect("fixture verification should run");

        assert!(
            result
                .comparison
                .to_string()
                .contains("final checksum mismatch")
        );
    }

    #[test]
    fn parity_summary_for_identical_runs_matches() {
        let first =
            record_counter_replay(60, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("first replay should succeed");
        let second =
            record_counter_replay(60, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("second replay should succeed");

        let comparison = compare_parity_summaries(
            &counter_parity_summary(&first.artifact).expect("summary should succeed"),
            &counter_parity_summary(&second.artifact).expect("summary should succeed"),
        );

        assert!(comparison.is_match());
        assert!(comparison.first_mismatch().is_none());
    }

    #[test]
    fn parity_comparison_catches_final_checksum_mismatch() {
        let recorded =
            record_counter_replay(61, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let expected = counter_parity_summary(&recorded.artifact).expect("summary should succeed");
        let mut actual = expected.clone();
        actual.final_checksum ^= 1;

        let comparison = compare_parity_summaries(&expected, &actual);

        assert!(matches!(
            comparison.first_mismatch(),
            Some(ParityMismatch::FinalChecksum { .. })
        ));
    }

    #[test]
    fn parity_comparison_catches_replay_digest_mismatch() {
        let recorded =
            record_counter_replay(62, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let expected = counter_parity_summary(&recorded.artifact).expect("summary should succeed");
        let mut actual = expected.clone();
        actual.replay_digest ^= 1;

        let comparison = compare_parity_summaries(&expected, &actual);

        assert!(matches!(
            comparison.first_mismatch(),
            Some(ParityMismatch::ReplayDigest { .. })
        ));
    }

    #[test]
    fn parity_comparison_catches_snapshot_digest_mismatch() {
        let recorded =
            record_counter_replay(63, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let expected = counter_parity_summary(&recorded.artifact).expect("summary should succeed");
        let mut actual = expected.clone();
        actual.snapshot_digest = Some(actual.snapshot_digest.expect("snapshot digest") ^ 1);

        let comparison = compare_parity_summaries(&expected, &actual);

        assert!(matches!(
            comparison.first_mismatch(),
            Some(ParityMismatch::SnapshotDigest { .. })
        ));
    }

    #[test]
    fn parity_comparison_output_is_clear() {
        let recorded =
            record_counter_replay(64, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("record mode should succeed");
        let expected: ParityArtifactSummary =
            counter_parity_summary(&recorded.artifact).expect("summary should succeed");
        let mut actual = expected.clone();
        actual.replay_digest ^= 1;

        let comparison = compare_parity_summaries(&expected, &actual);

        assert!(comparison.to_string().contains("replay digest mismatch"));
    }

    #[test]
    fn golden_fixture_codec_roundtrip_matches_wrapper_surface() {
        let fixture = generate_counter_golden_fixture(
            65,
            SnapshotPolicy::Every { interval: 2 },
            &sample_frames(),
        )
        .expect("fixture generation should succeed");
        let codec = counter_golden_fixture_codec();
        let bytes = codec
            .encode(&fixture)
            .expect("fixture export should succeed");
        let imported = codec.decode(&bytes).expect("fixture import should succeed");

        assert_eq!(fixture, imported);
    }
}
