#![forbid(unsafe_code)]

pub mod api;
pub mod bindings;
mod canonical;
pub mod config;
pub mod core;
pub mod deterministic;
pub mod engine;
pub mod fixture;
pub mod input;
pub mod parity;
pub mod persistence;
pub mod phases;
pub mod replay;
pub mod rng;
pub mod scenario;
pub mod scheduler;
pub mod serialization;
pub mod state;
pub mod validation;

pub use api::{
    COUNTER_ENGINE_FAMILY, CounterArtifactDigests, CounterCommand, CounterConfig,
    CounterConfigArtifact, CounterConfigArtifactCodec, CounterEngine, CounterFixtureInteropBundle,
    CounterGoldenFixture, CounterGoldenFixtureCodec, CounterGoldenFixtureResult,
    CounterInteropArtifacts, CounterParitySummary, CounterRecordedReplay, CounterReplayArtifact,
    CounterReplayArtifactCodec, CounterReplayLog, CounterReplayResult, CounterScenario,
    CounterScenarioCodec, CounterScenarioExecutionResult, CounterScenarioInteropBundle,
    CounterScenarioVerificationResult, CounterScheduler, CounterSnapshotArtifact,
    CounterSnapshotArtifactCodec, CounterStateValidator, EngineApi, build_counter_config_artifact,
    build_counter_scenario, counter_config_artifact_codec, counter_config_artifact_digest,
    counter_config_with_policy, counter_engine_with_config, counter_engine_with_policy,
    counter_golden_fixture_codec, counter_golden_fixture_digest, counter_parity_summary,
    counter_replay_artifact_codec, counter_replay_artifact_digest, counter_replay_summary,
    counter_scenario_codec, counter_scenario_digest, counter_snapshot_artifact_at_tick,
    counter_snapshot_artifact_codec, counter_snapshot_artifact_digest,
    counter_snapshot_artifact_from_engine, default_counter_config, execute_counter_scenario,
    execute_counter_scenario_interop_bundle, export_counter_config_artifact,
    export_counter_fixture_interop_bundle, export_counter_golden_fixture,
    export_counter_replay_artifact, export_counter_scenario, export_counter_snapshot_artifact,
    generate_counter_golden_fixture, generate_counter_golden_fixture_from_scenario,
    generate_counter_golden_fixture_with_config, import_counter_config_artifact,
    import_counter_golden_fixture, import_counter_replay_artifact, import_counter_scenario,
    import_counter_snapshot_artifact, inspect_counter_replay, minimal_counter_engine,
    record_counter_replay, record_counter_replay_with_config, resume_counter_replay_from_snapshot,
    resume_counter_replay_from_snapshot_with_config, verify_counter_golden_fixture,
    verify_counter_replay, verify_counter_replay_with_config, verify_counter_scenario,
};
pub use bindings::EngineBinding;
pub use config::{
    CONFIG_ARTIFACT_SCHEMA_VERSION, ConfigArtifact, ConfigArtifactMetadata,
    ConfigArtifactSerializer, ConfigIdentity, CounterSimulationConfig, SimulationConfig,
};
pub use core::{EngineError, Seed, Tick};
pub use deterministic::{DeterministicList, DeterministicMap};
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
pub use scenario::{
    SCENARIO_ARTIFACT_SCHEMA_VERSION, ScenarioExecutionResult, ScenarioVerificationResult,
    SimulationScenario, SimulationScenarioMetadata, SimulationScenarioSerializer,
};
pub use scheduler::{FixedScheduler, PhaseDescriptor, PhaseGroup, Scheduler};
pub use serialization::{
    CounterCommandTextSerializer, CounterConfigTextSerializer, CounterSnapshotTextSerializer,
    SerializationError, Serializer,
};
pub use state::{
    CounterEntityInit, CounterEntitySnapshot, CounterSnapshot, CounterState, EntityId,
    SimulationState,
};
pub use validation::{
    NoopStateValidator, StateDigestProgression, StateValidator, ValidationCheckpoint,
    ValidationContext, ValidationPolicy, ValidationSummary,
};

#[cfg(test)]
mod tests {
    use crate::api::{
        CounterCommand, build_counter_config_artifact, build_counter_engine,
        build_counter_scenario, counter_golden_fixture_codec, counter_parity_summary,
        counter_replay_artifact_codec, counter_replay_artifact_digest, counter_replay_summary,
        counter_scenario_digest, counter_snapshot_artifact_digest, default_counter_config,
        default_counter_scheduler, execute_counter_scenario,
        execute_counter_scenario_interop_bundle, export_counter_config_artifact,
        export_counter_fixture_interop_bundle, export_counter_golden_fixture,
        export_counter_replay_artifact, export_counter_scenario, export_counter_snapshot_artifact,
        generate_counter_golden_fixture, generate_counter_golden_fixture_from_scenario,
        generate_counter_golden_fixture_with_config, import_counter_config_artifact,
        import_counter_golden_fixture, import_counter_replay_artifact, import_counter_scenario,
        import_counter_snapshot_artifact, inspect_counter_replay, minimal_counter_engine,
        record_counter_replay, record_counter_replay_with_config, reordered_counter_scheduler,
        resume_counter_replay_from_snapshot, verify_counter_golden_fixture, verify_counter_replay,
        verify_counter_replay_with_config, verify_counter_scenario,
    };
    use crate::core::EngineError;
    use crate::engine::{Engine, SnapshotPolicy};
    use crate::input::InputFrame;
    use crate::parity::{ParityArtifactSummary, ParityMismatch, compare_parity_summaries};
    use crate::replay::{ReplayLog, ReplayMismatchKind, compare_replay_traces};
    use crate::rng::{Rng, SplitMix64};
    use crate::scheduler::{PhaseGroup, Scheduler};
    use crate::serialization::{CounterSnapshotTextSerializer, Serializer};
    use crate::state::{CounterEntityInit, CounterState, EntityId, SimulationState};
    use crate::validation::{ValidationCheckpoint, ValidationPolicy};

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

    fn sample_config(snapshot_policy: SnapshotPolicy) -> crate::api::CounterConfig {
        crate::api::CounterConfig {
            snapshot_policy,
            ..default_counter_config()
        }
    }

    fn entity_config(
        snapshot_policy: SnapshotPolicy,
        extra_entities: &[(i64, i64)],
    ) -> crate::api::CounterConfig {
        let mut config = sample_config(snapshot_policy);
        config.initial_entities = extra_entities
            .iter()
            .map(|(value, velocity)| CounterEntityInit {
                value: *value,
                velocity: *velocity,
            })
            .collect();
        config
    }

    fn seeded_frames(seed: u64, count: usize) -> Vec<InputFrame<CounterCommand>> {
        let mut rng = SplitMix64::from_seed(seed);
        (1..=count)
            .map(|tick| {
                InputFrame::new(
                    tick as u64,
                    CounterCommand {
                        delta: i64::try_from((rng.next_u64() % 11) as i64 - 5)
                            .expect("bounded delta should fit i64"),
                        consume_entropy: rng.next_u64() & 1 == 0,
                    },
                )
            })
            .collect()
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
        let tampered = text.replace("artifact_schema_version=2", "artifact_schema_version=9");

        let error = import_counter_replay_artifact(tampered.as_bytes())
            .expect_err("unsupported replay version should fail");

        assert_eq!(
            error,
            EngineError::UnsupportedSchemaVersion {
                artifact: "replay artifact",
                expected: 2,
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
            "snapshot_payload_schema_version=2",
            "snapshot_payload_schema_version=9",
        );

        let error = import_counter_snapshot_artifact(tampered.as_bytes())
            .expect_err("unsupported snapshot version should fail");

        assert_eq!(
            error,
            EngineError::UnsupportedSchemaVersion {
                artifact: "snapshot payload",
                expected: 2,
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
            "engine_family=xenor-engine-rust/counter\nbase_seed=52\nfinal_tick=4\nfinal_checksum=5099580070206651932\nconfig_payload_schema_version=2\nconfig_digest=5311596323007274562\nreplay_artifact_schema_version=2\nsnapshot_artifact_schema_version=2\ncommand_payload_schema_version=1\nsnapshot_payload_schema_version=2\nreplay_digest=993437642736629535\nsnapshot_digest=7556615425519769554\nscenario_digest=none\n"
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
                .contains("validation summary mismatch")
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
            .entities
            .get_mut(0)
            .expect("primary entity should exist in snapshot")
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

    #[test]
    fn counter_state_entity_insertion_order_is_stable() {
        let state = CounterState::with_initial_entities(
            5,
            1,
            &vec![
                CounterEntityInit {
                    value: 11,
                    velocity: 2,
                },
                CounterEntityInit {
                    value: -3,
                    velocity: -1,
                },
            ]
            .into_iter()
            .collect(),
        );

        assert_eq!(
            state.entity_ids().collect::<Vec<_>>(),
            vec![EntityId(1), EntityId(2), EntityId(3)]
        );
        assert_eq!(
            state
                .entity_snapshots()
                .iter()
                .map(|entity| entity.value)
                .collect::<Vec<_>>(),
            vec![5, 11, -3]
        );
    }

    #[test]
    fn snapshot_with_entities_roundtrips_deterministically() {
        let config = entity_config(SnapshotPolicy::Every { interval: 2 }, &[(4, 1), (-8, 3)]);
        let recorded = record_counter_replay_with_config(&config, 182, &sample_frames())
            .expect("recording should succeed");
        let snapshot = crate::api::counter_snapshot_artifact_at_tick(&recorded.artifact, 2)
            .expect("tick 2 snapshot should exist");

        let bytes = export_counter_snapshot_artifact(&snapshot)
            .expect("snapshot artifact should serialize");
        let imported =
            import_counter_snapshot_artifact(&bytes).expect("snapshot artifact should deserialize");

        assert_eq!(snapshot, imported);
        assert_eq!(imported.payload.entities.len(), 3);
    }

    #[test]
    fn replay_with_entities_remains_identical_across_runs() {
        let config = entity_config(SnapshotPolicy::Every { interval: 2 }, &[(7, 2), (9, -1)]);
        let first = record_counter_replay_with_config(&config, 183, &sample_frames())
            .expect("first recording should succeed");
        let second = record_counter_replay_with_config(&config, 183, &sample_frames())
            .expect("second recording should succeed");

        assert_eq!(first.artifact, second.artifact);
        assert_eq!(first.result.summary, second.result.summary);
    }

    #[test]
    fn fixture_from_scenario_reports_scenario_category_when_replay_contract_is_tampered() {
        let config = entity_config(SnapshotPolicy::Every { interval: 2 }, &[(1, 1)]);
        let scenario = build_counter_scenario(&config, 184, &sample_frames(), None)
            .expect("scenario should build");
        let mut fixture = generate_counter_golden_fixture_from_scenario(&scenario)
            .expect("fixture generation from scenario should succeed");
        fixture.replay_artifact.records[1].input.command.delta += 1;

        let result =
            verify_counter_golden_fixture(&fixture).expect("fixture verification should run");

        assert!(!result.passed());
        assert!(result.scenario_mismatch.is_some());
        assert!(
            result
                .scenario_mismatch
                .as_deref()
                .expect("scenario mismatch should exist")
                .contains("scenario frames mismatch")
        );
    }

    #[test]
    fn fixture_verification_reports_summary_category() {
        let fixture = generate_counter_golden_fixture(
            185,
            SnapshotPolicy::Every { interval: 2 },
            &sample_frames(),
        )
        .expect("fixture generation should succeed");
        let mut tampered = fixture.clone();
        tampered.summary.replay_digest ^= 1;

        let result =
            verify_counter_golden_fixture(&tampered).expect("fixture verification should run");

        assert!(!result.passed());
        assert!(result.summary_mismatch.is_some());
        assert!(
            result
                .summary_mismatch
                .as_deref()
                .expect("summary mismatch should exist")
                .contains("replay digest mismatch")
        );
    }

    #[test]
    fn interop_scenario_bundle_exports_consistent_bytes_and_digests() {
        let config = entity_config(SnapshotPolicy::Every { interval: 2 }, &[(6, 0), (2, 1)]);
        let scenario = build_counter_scenario(&config, 186, &sample_frames(), None)
            .expect("scenario should build");
        let bundle = execute_counter_scenario_interop_bundle(&scenario)
            .expect("interop scenario bundle should execute");

        let replay = import_counter_replay_artifact(&bundle.artifacts.replay_artifact)
            .expect("replay artifact should import");
        let snapshot = bundle
            .artifacts
            .snapshot_artifact
            .as_ref()
            .map(|bytes| import_counter_snapshot_artifact(bytes))
            .transpose()
            .expect("snapshot artifact should import");

        assert_eq!(
            bundle.digests.replay_artifact,
            counter_replay_artifact_digest(&replay).expect("replay digest should compute")
        );
        assert_eq!(
            bundle.digests.snapshot_artifact,
            snapshot
                .as_ref()
                .map(counter_snapshot_artifact_digest)
                .transpose()
                .expect("snapshot digest should compute")
        );
        assert_eq!(
            bundle.digests.scenario_artifact,
            Some(counter_scenario_digest(&scenario).expect("scenario digest should compute"))
        );
    }

    #[test]
    fn interop_fixture_bundle_exports_consistent_bytes_and_digests() {
        let config = entity_config(SnapshotPolicy::Every { interval: 2 }, &[(3, 2)]);
        let scenario = build_counter_scenario(&config, 187, &sample_frames(), None)
            .expect("scenario should build");
        let fixture = generate_counter_golden_fixture_from_scenario(&scenario)
            .expect("fixture should generate");
        let bundle =
            export_counter_fixture_interop_bundle(&fixture).expect("fixture bundle should export");
        let imported = import_counter_golden_fixture(
            bundle
                .artifacts
                .golden_fixture
                .as_ref()
                .expect("fixture bytes should exist"),
        )
        .expect("fixture should import");

        assert_eq!(fixture, imported);
        assert_eq!(
            bundle.digests.replay_artifact,
            counter_replay_artifact_digest(&fixture.replay_artifact)
                .expect("replay digest should compute")
        );
        assert_eq!(
            bundle.digests.scenario_artifact,
            fixture
                .scenario_artifact
                .as_ref()
                .map(counter_scenario_digest)
                .transpose()
                .expect("scenario digest should compute")
        );
    }

    #[test]
    fn long_scenario_replay_is_identical_across_runs() {
        let config = entity_config(
            SnapshotPolicy::Every { interval: 16 },
            &[(12, 1), (-4, 0), (7, -2)],
        );
        let frames = seeded_frames(404, 128);
        let first = record_counter_replay_with_config(&config, 188, &frames)
            .expect("first long replay should succeed");
        let second = record_counter_replay_with_config(&config, 188, &frames)
            .expect("second long replay should succeed");

        assert_eq!(first.artifact, second.artifact);
        compare_replay_traces(
            first.artifact.records.as_slice(),
            second.artifact.records.as_slice(),
        )
        .expect("long deterministic traces should match");
    }

    #[test]
    fn resume_from_mid_snapshot_matches_long_full_run() {
        let config = entity_config(SnapshotPolicy::Every { interval: 10 }, &[(1, 0), (2, 1)]);
        let frames = seeded_frames(505, 120);
        let recorded = record_counter_replay_with_config(&config, 189, &frames)
            .expect("recording should succeed");
        let snapshot = crate::api::counter_snapshot_artifact_at_tick(&recorded.artifact, 100)
            .expect("snapshot at tick 100 should exist");

        let full = verify_counter_replay_with_config(&config, &recorded.artifact)
            .expect("full replay verification should succeed");
        let resumed = crate::api::resume_counter_replay_from_snapshot_with_config(
            &config,
            &snapshot,
            &recorded.artifact,
        )
        .expect("resume verification should succeed");

        assert_eq!(full.final_tick, resumed.final_tick);
        assert_eq!(full.final_checksum, resumed.final_checksum);
        assert_eq!(full.summary, resumed.summary);
    }

    #[test]
    fn seeded_random_input_replay_is_deterministic() {
        let config = entity_config(SnapshotPolicy::Never, &[(8, 1)]);
        let frames = seeded_frames(606, 64);
        let first = record_counter_replay_with_config(&config, 190, &frames)
            .expect("first replay should succeed");
        let second = record_counter_replay_with_config(&config, 190, &frames)
            .expect("second replay should succeed");

        assert_eq!(
            counter_replay_artifact_digest(&first.artifact).expect("digest should compute"),
            counter_replay_artifact_digest(&second.artifact).expect("digest should compute")
        );
    }

    #[test]
    fn multiple_identical_runs_keep_same_replay_digest() {
        let config = entity_config(SnapshotPolicy::Every { interval: 8 }, &[(5, 0), (9, 2)]);
        let frames = seeded_frames(707, 96);
        let baseline = record_counter_replay_with_config(&config, 191, &frames)
            .expect("baseline replay should succeed");
        let expected_digest =
            counter_replay_artifact_digest(&baseline.artifact).expect("digest should compute");

        for _ in 0..5 {
            let recorded = record_counter_replay_with_config(&config, 191, &frames)
                .expect("loop replay should succeed");
            assert_eq!(
                counter_replay_artifact_digest(&recorded.artifact).expect("digest should compute"),
                expected_digest
            );
        }
    }

    #[test]
    fn parity_summary_stays_stable_for_entity_config_across_runs() {
        let config = entity_config(SnapshotPolicy::Every { interval: 4 }, &[(10, 1), (-2, -1)]);
        let frames = seeded_frames(808, 48);
        let first = record_counter_replay_with_config(&config, 192, &frames)
            .expect("first replay should succeed");
        let second = record_counter_replay_with_config(&config, 192, &frames)
            .expect("second replay should succeed");

        assert_eq!(
            counter_parity_summary(&first.artifact).expect("first parity summary should work"),
            counter_parity_summary(&second.artifact).expect("second parity summary should work")
        );
    }

    #[test]
    fn config_artifact_export_import_export_is_canonical_and_identical() {
        let mut config = sample_config(SnapshotPolicy::Every { interval: 2 });
        config.initial_value = 9;
        config.initial_velocity = -2;
        config.validation_policy = ValidationPolicy::EveryPhase;
        let artifact =
            build_counter_config_artifact(&config).expect("config artifact should build");

        let first =
            export_counter_config_artifact(&artifact).expect("config artifact should serialize");
        let imported =
            import_counter_config_artifact(&first).expect("config artifact should deserialize");
        let second =
            export_counter_config_artifact(&imported).expect("config artifact should re-serialize");

        assert_eq!(artifact, imported);
        assert_eq!(first, second);
    }

    #[test]
    fn scenario_export_import_export_is_canonical_and_identical() {
        let mut config = sample_config(SnapshotPolicy::Every { interval: 2 });
        config.initial_value = 4;
        config.validation_policy = ValidationPolicy::EveryPhase;
        let scenario = build_counter_scenario(&config, 170, &sample_frames(), None)
            .expect("scenario should build");

        let first = export_counter_scenario(&scenario).expect("scenario should serialize");
        let imported = import_counter_scenario(&first).expect("scenario should deserialize");
        let second = export_counter_scenario(&imported).expect("scenario should re-serialize");

        assert_eq!(scenario, imported);
        assert_eq!(first, second);
    }

    #[test]
    fn config_digest_is_stable_for_identical_config() {
        let mut config = sample_config(SnapshotPolicy::Never);
        config.initial_value = 3;
        config.max_abs_value = 500;

        let first = build_counter_config_artifact(&config).expect("first config should build");
        let second = build_counter_config_artifact(&config).expect("second config should build");

        assert_eq!(first.metadata.identity, second.metadata.identity);
    }

    #[test]
    fn config_mismatch_fails_replay_verification() {
        let mut config = sample_config(SnapshotPolicy::Every { interval: 2 });
        config.initial_value = 12;
        let recorded = record_counter_replay_with_config(&config, 171, &sample_frames())
            .expect("recording with config should succeed");

        let wrong_config = sample_config(SnapshotPolicy::Every { interval: 2 });
        let error = verify_counter_replay_with_config(&wrong_config, &recorded.artifact)
            .expect_err("wrong config should fail replay verification");

        assert!(matches!(error, EngineError::ConfigMismatch { .. }));
        assert!(
            error
                .to_string()
                .contains("replay config identity mismatch")
        );
    }

    #[test]
    fn config_mismatch_fails_golden_fixture_verification() {
        let mut config = sample_config(SnapshotPolicy::Every { interval: 2 });
        config.initial_velocity = 5;
        let mut fixture =
            generate_counter_golden_fixture_with_config(&config, 172, &sample_frames())
                .expect("fixture generation should succeed");

        let mut wrong_config = config.clone();
        wrong_config.initial_velocity += 1;
        fixture.config_artifact =
            build_counter_config_artifact(&wrong_config).expect("wrong config should build");

        let result =
            verify_counter_golden_fixture(&fixture).expect("fixture verification should run");

        assert!(!result.passed());
        assert!(result.config_mismatch.is_some());
        assert!(
            result
                .config_mismatch
                .as_deref()
                .expect("config mismatch should be present")
                .contains("replay config identity mismatch")
        );
    }

    #[test]
    fn invariant_violation_is_reported_at_after_simulation_group_boundary() {
        let mut config = sample_config(SnapshotPolicy::Never);
        config.validation_policy = ValidationPolicy::EveryPhase;
        config.max_abs_value = 1;

        let error = record_counter_replay_with_config(&config, 173, &sample_frames())
            .expect_err("tight config should violate invariant");

        assert!(matches!(
            error,
            EngineError::InvariantViolation {
                tick: 1,
                checkpoint: "after_simulation_group",
                ..
            }
        ));
        assert!(
            error
                .to_string()
                .contains("value exceeded deterministic limit")
        );
    }

    #[test]
    fn validation_policy_tick_boundary_only_records_boundary_checkpoints() {
        let config = sample_config(SnapshotPolicy::Never);
        let recorded = record_counter_replay_with_config(&config, 174, &sample_frames())
            .expect("recording should succeed");

        let view = inspect_counter_replay(&recorded.artifact);
        for tick_summary in &view.tick_summaries {
            assert_eq!(tick_summary.validation_summaries.len(), 2);
            assert_eq!(
                tick_summary.validation_summaries[0].checkpoint,
                ValidationCheckpoint::BeforeTickBegin
            );
            assert_eq!(
                tick_summary.validation_summaries[1].checkpoint,
                ValidationCheckpoint::AfterFinalize
            );
        }
    }

    #[test]
    fn validation_policy_every_phase_records_all_checkpoints() {
        let mut config = sample_config(SnapshotPolicy::Never);
        config.validation_policy = ValidationPolicy::EveryPhase;
        let recorded = record_counter_replay_with_config(&config, 175, &sample_frames())
            .expect("recording should succeed");

        let view = inspect_counter_replay(&recorded.artifact);
        for tick_summary in &view.tick_summaries {
            assert_eq!(tick_summary.validation_summaries.len(), 4);
            assert_eq!(
                tick_summary
                    .validation_summaries
                    .iter()
                    .map(|summary| summary.checkpoint)
                    .collect::<Vec<_>>(),
                vec![
                    ValidationCheckpoint::BeforeTickBegin,
                    ValidationCheckpoint::AfterInputApplied,
                    ValidationCheckpoint::AfterSimulationGroup,
                    ValidationCheckpoint::AfterFinalize,
                ]
            );
        }
    }

    #[test]
    fn invalid_config_payload_schema_version_fails_fast() {
        let artifact = build_counter_config_artifact(&sample_config(SnapshotPolicy::Never))
            .expect("config artifact should build");
        let bytes =
            export_counter_config_artifact(&artifact).expect("config artifact should serialize");
        let text = String::from_utf8(bytes).expect("config bytes should be utf8");
        let tampered = text.replace(
            "config_payload_schema_version=2",
            "config_payload_schema_version=9",
        );

        let error = import_counter_config_artifact(tampered.as_bytes())
            .expect_err("unsupported config payload version should fail");

        assert_eq!(
            error,
            EngineError::UnsupportedSchemaVersion {
                artifact: "config payload",
                expected: 2,
                got: 9,
            }
        );
    }

    #[test]
    fn scenario_execution_is_stable_and_sets_scenario_digest() {
        let mut config = sample_config(SnapshotPolicy::Every { interval: 2 });
        config.initial_value = 7;
        config.validation_policy = ValidationPolicy::EveryPhase;
        let scenario = build_counter_scenario(&config, 176, &sample_frames(), None)
            .expect("scenario should build");

        let first = execute_counter_scenario(&scenario).expect("first execution should succeed");
        let second = execute_counter_scenario(&scenario).expect("second execution should succeed");

        assert_eq!(first.scenario_digest, second.scenario_digest);
        assert_eq!(first.parity_summary, second.parity_summary);
        assert_eq!(first.inspection, second.inspection);
        assert_eq!(
            first.parity_summary.scenario_digest,
            Some(first.scenario_digest)
        );
    }

    #[test]
    fn scenario_verification_uses_expected_parity_summary() {
        let config = sample_config(SnapshotPolicy::Every { interval: 2 });
        let base = build_counter_scenario(&config, 177, &sample_frames(), None)
            .expect("scenario should build");
        let execution = execute_counter_scenario(&base).expect("scenario execution should succeed");
        let scenario = build_counter_scenario(
            &config,
            177,
            &sample_frames(),
            Some(execution.parity_summary.clone()),
        )
        .expect("scenario with expectation should build");

        let verification =
            verify_counter_scenario(&scenario).expect("scenario verification should succeed");

        assert!(verification.passed());
        assert!(
            verification
                .parity_comparison
                .expect("comparison should exist")
                .is_match()
        );
    }

    #[test]
    fn generate_fixture_from_scenario_preserves_scenario_digest_and_verifies() {
        let mut config = sample_config(SnapshotPolicy::Every { interval: 2 });
        config.initial_velocity = 3;
        let scenario = build_counter_scenario(&config, 178, &sample_frames(), None)
            .expect("scenario should build");
        let fixture = generate_counter_golden_fixture_from_scenario(&scenario)
            .expect("fixture generation from scenario should succeed");

        assert!(fixture.scenario_artifact.is_some());
        assert!(fixture.summary.scenario_digest.is_some());

        let result =
            verify_counter_golden_fixture(&fixture).expect("fixture verification should run");

        assert!(result.passed());
        assert!(result.config_mismatch.is_none());
    }

    #[test]
    fn scenario_runner_final_snapshot_matches_direct_execution() {
        let config = sample_config(SnapshotPolicy::Every { interval: 2 });
        let scenario = build_counter_scenario(&config, 179, &sample_frames(), None)
            .expect("scenario should build");
        let scenario_execution =
            execute_counter_scenario(&scenario).expect("scenario execution should succeed");
        let direct = record_counter_replay_with_config(&config, 179, &sample_frames())
            .expect("direct replay should succeed");

        assert_eq!(
            scenario_execution.final_snapshot,
            direct.result.final_snapshot
        );
        assert_eq!(
            scenario_execution.replay.result.summary.final_checksum,
            direct.result.summary.final_checksum
        );
    }

    #[test]
    fn parity_comparison_catches_config_digest_mismatch() {
        let mut config = sample_config(SnapshotPolicy::Every { interval: 2 });
        config.initial_value = 6;
        let recorded = record_counter_replay_with_config(&config, 180, &sample_frames())
            .expect("recording should succeed");
        let expected = counter_parity_summary(&recorded.artifact).expect("summary should succeed");
        let mut actual = expected.clone();
        actual.config_digest ^= 1;

        let comparison = compare_parity_summaries(&expected, &actual);

        assert!(matches!(
            comparison.first_mismatch(),
            Some(ParityMismatch::ConfigDigest { .. })
        ));
    }

    #[test]
    fn parity_comparison_catches_config_schema_version_mismatch() {
        let recorded =
            record_counter_replay(181, SnapshotPolicy::Every { interval: 2 }, &sample_frames())
                .expect("recording should succeed");
        let expected = counter_parity_summary(&recorded.artifact).expect("summary should succeed");
        let mut actual = expected.clone();
        actual.config_payload_schema_version += 1;

        let comparison = compare_parity_summaries(&expected, &actual);

        assert!(matches!(
            comparison.first_mismatch(),
            Some(ParityMismatch::ConfigSchemaVersion { .. })
        ));
    }
}
