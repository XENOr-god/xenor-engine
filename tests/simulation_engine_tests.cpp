#include <algorithm>
#include <chrono>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <vector>

#include <catch2/catch_test_macros.hpp>

#include "xenor/xenor.hpp"

namespace {

struct CounterState final : xenor::SimulationState {
  std::int64_t value{0};
  std::vector<std::string> execution_trace;
};

struct PipelineState final : xenor::SimulationState {
  std::uint64_t ore{0};
  std::uint64_t ingots{0};
  std::uint64_t finished_units{0};
};

struct CounterPayload {
  std::int64_t value{0};
  std::vector<std::string> execution_trace;

  bool operator==(const CounterPayload&) const = default;
};

struct PipelinePayload {
  std::uint64_t ore{0};
  std::uint64_t ingots{0};
  std::uint64_t finished_units{0};

  bool operator==(const PipelinePayload&) const = default;
};

struct WorkloadInput {
  std::int64_t supply{0};
  std::int64_t demand{0};

  bool operator==(const WorkloadInput&) const = default;
};

struct SeededInputState final : xenor::SimulationState {
  std::int64_t inventory{0};
  std::int64_t shipped{0};
  std::int64_t backlog{0};
  std::uint64_t random_checksum{0};
  std::vector<std::string> input_trace;
};

bool identical(const CounterState& left, const CounterState& right) {
  return left.value == right.value &&
         left.execution_trace == right.execution_trace &&
         left.last_completed_tick() == right.last_completed_tick();
}

bool identical(const PipelineState& left, const PipelineState& right) {
  return left.ore == right.ore && left.ingots == right.ingots &&
         left.finished_units == right.finished_units &&
         left.last_completed_tick() == right.last_completed_tick();
}

bool identical(const SeededInputState& left, const SeededInputState& right) {
  return left.inventory == right.inventory &&
         left.shipped == right.shipped &&
         left.backlog == right.backlog &&
         left.random_checksum == right.random_checksum &&
         left.input_trace == right.input_trace &&
         left.last_completed_tick() == right.last_completed_tick();
}

struct CounterSnapshotAdapter {
  using payload_type = CounterPayload;
  static constexpr xenor::snapshot_boundary_payload_version_type
      current_payload_version = 2;

  std::size_t capture_calls{0};
  std::size_t supports_calls{0};
  std::size_t restore_calls{0};

  xenor::snapshot_boundary_payload_version_type payload_version() const {
    return current_payload_version;
  }

  bool supports_payload_version(
      xenor::snapshot_boundary_payload_version_type payload_version) {
    ++supports_calls;
    return payload_version == current_payload_version;
  }

  payload_type capture(const CounterState& state) {
    ++capture_calls;
    return payload_type{
        .value = state.value,
        .execution_trace = state.execution_trace,
    };
  }

  CounterState restore(const payload_type& payload,
                       xenor::snapshot_boundary_payload_version_type payload_version) {
    ++restore_calls;
    if (payload_version != current_payload_version) {
      throw std::invalid_argument("unexpected counter payload version");
    }

    CounterState state;
    state.value = payload.value;
    state.execution_trace = payload.execution_trace;
    return state;
  }
};

struct PipelineSnapshotAdapter {
  using payload_type = PipelinePayload;
  static constexpr xenor::snapshot_boundary_payload_version_type
      current_payload_version = 1;

  xenor::snapshot_boundary_payload_version_type payload_version() const {
    return current_payload_version;
  }

  bool supports_payload_version(
      xenor::snapshot_boundary_payload_version_type payload_version) const {
    return payload_version == current_payload_version;
  }

  payload_type capture(const PipelineState& state) {
    return payload_type{
        .ore = state.ore,
        .ingots = state.ingots,
        .finished_units = state.finished_units,
    };
  }

  PipelineState restore(const payload_type& payload,
                        xenor::snapshot_boundary_payload_version_type payload_version) {
    if (payload_version != current_payload_version) {
      throw std::invalid_argument("unexpected pipeline payload version");
    }

    PipelineState state;
    state.ore = payload.ore;
    state.ingots = payload.ingots;
    state.finished_units = payload.finished_units;
    return state;
  }
};

struct MigratingCounterSnapshotAdapter {
  using payload_type = CounterPayload;
  static constexpr xenor::snapshot_boundary_payload_version_type
      current_payload_version = 2;

  std::size_t supports_calls{0};
  std::size_t restore_calls{0};

  xenor::snapshot_boundary_payload_version_type payload_version() const {
    return current_payload_version;
  }

  bool supports_payload_version(
      xenor::snapshot_boundary_payload_version_type payload_version) {
    ++supports_calls;
    return payload_version == 1 || payload_version == current_payload_version;
  }

  payload_type capture(const CounterState& state) {
    return payload_type{
        .value = state.value,
        .execution_trace = state.execution_trace,
    };
  }

  CounterState restore(const payload_type& payload,
                       xenor::snapshot_boundary_payload_version_type payload_version) {
    ++restore_calls;

    CounterState state;
    state.execution_trace = payload.execution_trace;
    if (payload_version == 1) {
      state.value = payload.value;
      state.execution_trace.push_back("migrated:v1");
      return state;
    }

    if (payload_version == current_payload_version) {
      state.value = payload.value;
      state.execution_trace.push_back("restored:v2");
      return state;
    }

    throw std::invalid_argument("unsupported migrating counter payload version");
  }
};

struct MetadataOwnershipSnapshotAdapter {
  using payload_type = CounterPayload;
  static constexpr xenor::snapshot_boundary_payload_version_type
      current_payload_version = 1;

  xenor::snapshot_boundary_payload_version_type payload_version() const {
    return current_payload_version;
  }

  bool supports_payload_version(
      xenor::snapshot_boundary_payload_version_type payload_version) const {
    return payload_version == current_payload_version;
  }

  payload_type capture(const CounterState& state) {
    return payload_type{
        .value = state.value,
        .execution_trace = state.execution_trace,
    };
  }

  CounterState restore(const payload_type& payload,
                       xenor::snapshot_boundary_payload_version_type payload_version) {
    if (payload_version != current_payload_version) {
      throw std::invalid_argument("unexpected metadata ownership payload version");
    }

    CounterState state;
    state.value = payload.value;
    state.execution_trace = payload.execution_trace;
    state.set_last_completed_tick(999);
    return state;
  }
};

template <typename State>
bool identical(const xenor::SimulationSnapshot<State>& left,
               const xenor::SimulationSnapshot<State>& right) {
  return left.tick == right.tick && left.elapsed == right.elapsed &&
         left.seed == right.seed && identical(left.state, right.state);
}

auto make_pipeline_engine() {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<PipelineState> engine{xenor::SimulationConfig{25ms}};

  engine.add_system(xenor::SystemPhase::PreUpdate,
                    "mining",
                    [](PipelineState& state, const xenor::StepContext&) {
    state.ore += 4;
  });

  engine.add_system(xenor::SystemPhase::Update,
                    "smelting",
                    [](PipelineState& state, const xenor::StepContext&) {
    const auto ingot_batches = state.ore / 2;
    state.ore -= ingot_batches * 2;
    state.ingots += ingot_batches;
  });

  engine.add_system(xenor::SystemPhase::PostUpdate,
                    "packing",
                    [](PipelineState& state, const xenor::StepContext&) {
    const auto finished_batches = state.ingots / 3;
    state.ingots -= finished_batches * 3;
    state.finished_units += finished_batches;
  });

  return engine;
}

auto make_seeded_input_engine(xenor::SimulationConfig::seed_type seed) {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<SeededInputState, WorkloadInput> engine{
      xenor::SimulationConfig{5ms, seed}};

  engine.add_system(
      xenor::SystemPhase::PreUpdate,
      "record_input",
      [](SeededInputState& state, const xenor::InputStepContext<WorkloadInput>& context) {
        const auto& input = context.input();
        state.input_trace.push_back(
            std::to_string(context.tick) + ":" +
            std::to_string(input.supply) + ":" +
            std::to_string(input.demand));
      });

  engine.add_system(
      xenor::SystemPhase::PreUpdate,
      "receive_supply",
      [](SeededInputState& state, const xenor::InputStepContext<WorkloadInput>& context) {
        state.inventory += context.input().supply;
      });

  engine.add_system(
      xenor::SystemPhase::Update,
      "seeded_adjustment",
      [](SeededInputState& state, const xenor::InputStepContext<WorkloadInput>& context) {
        const auto random_value = context.rng().next_u64();
        state.random_checksum ^= random_value + context.step_seed;
        state.inventory += static_cast<std::int64_t>(random_value % 3ULL);
      });

  engine.add_system(
      xenor::SystemPhase::PostUpdate,
      "ship",
      [](SeededInputState& state, const xenor::InputStepContext<WorkloadInput>& context) {
        const auto& input = context.input();
        const auto shipped = std::min(state.inventory, input.demand);
        state.inventory -= shipped;
        state.shipped += shipped;
        state.backlog += input.demand - shipped;
      });

  return engine;
}

xenor::InputSequence<WorkloadInput> make_workload_inputs() {
  return xenor::InputSequence<WorkloadInput>{
      {{3, 1}, {1, 4}, {4, 2}, {2, 5}, {5, 3}, {1, 2}, {3, 4}, {2, 1}}};
}

xenor::InputSequence<WorkloadInput> make_variant_workload_inputs() {
  return xenor::InputSequence<WorkloadInput>{
      {{3, 1}, {1, 4}, {2, 2}, {2, 6}, {5, 3}, {1, 1}, {4, 4}, {2, 1}}};
}

template <typename Input>
std::size_t count_events(const xenor::ReplayTrace<Input>& trace,
                         xenor::ReplayEventKind kind) {
  return static_cast<std::size_t>(std::count_if(
      trace.events.begin(), trace.events.end(), [kind](const auto& event) {
        return event.kind == kind;
      }));
}

}  // namespace

TEST_CASE("SimulationConfig stores an explicit deterministic seed", "[config][seed]") {
  using namespace std::chrono_literals;

  const xenor::SimulationConfig config{4ms, 41};

  REQUIRE(config.tick_duration() == 4ms);
  REQUIRE(config.seed() == 41);
}

TEST_CASE("SimulationClock advances in fixed increments", "[clock]") {
  using namespace std::chrono_literals;

  xenor::SimulationClock clock{xenor::SimulationConfig{10ms, 7}};

  REQUIRE(clock.current_tick() == 0);
  REQUIRE(clock.elapsed_duration() == 0ms);

  clock.advance();
  REQUIRE(clock.current_tick() == 1);
  REQUIRE(clock.elapsed_duration() == 10ms);

  clock.advance_by(4);
  REQUIRE(clock.current_tick() == 5);
  REQUIRE(clock.elapsed_duration() == 50ms);
}

TEST_CASE("SimulationEngine advances tick metadata and captures seed-aware snapshots",
          "[engine][snapshot][seed]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms, 19}};
  engine.add_system("increment", [](CounterState& state, const xenor::StepContext& context) {
    state.value += static_cast<std::int64_t>(context.tick);
  });

  engine.run_for_ticks(3);

  REQUIRE(engine.seed() == 19);
  REQUIRE(engine.clock().current_tick() == 3);
  REQUIRE(engine.state().last_completed_tick() == 3);
  REQUIRE(engine.state().value == 6);

  const auto snapshot = engine.capture_snapshot();
  const auto compatibility_snapshot = engine.snapshot();

  REQUIRE(snapshot.tick == 3);
  REQUIRE(snapshot.elapsed == 3ms);
  REQUIRE(snapshot.seed == 19);
  REQUIRE(snapshot.state.value == 6);
  REQUIRE(snapshot.state.last_completed_tick() == 3);
  REQUIRE(identical(snapshot, compatibility_snapshot));
}

TEST_CASE("Snapshot boundary metadata matches the in-memory snapshot",
          "[snapshot][boundary]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms, 19}};
  engine.add_system("increment", [](CounterState& state, const xenor::StepContext& context) {
    state.value += static_cast<std::int64_t>(context.tick);
  });

  engine.run_for_ticks(3);

  CounterSnapshotAdapter adapter;
  const auto snapshot = engine.capture_snapshot();
  const auto metadata = xenor::make_snapshot_boundary_metadata(snapshot);
  const auto boundary = engine.capture_snapshot_boundary(adapter);

  REQUIRE(metadata == boundary.metadata);
  REQUIRE(boundary.metadata.engine_version ==
          xenor::current_snapshot_boundary_engine_version);
  REQUIRE(boundary.metadata.tick == snapshot.tick);
  REQUIRE(boundary.metadata.elapsed == snapshot.elapsed);
  REQUIRE(boundary.metadata.seed == snapshot.seed);
  REQUIRE(boundary.metadata.state_last_completed_tick ==
          snapshot.state.last_completed_tick());
  REQUIRE(boundary.payload_version == adapter.payload_version());
  REQUIRE(adapter.capture_calls == 1);
}

TEST_CASE("Snapshot boundary adapters are invoked for capture and restore",
          "[snapshot][boundary]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> source{xenor::SimulationConfig{1ms, 19}};
  source.add_system("increment", [](CounterState& state, const xenor::StepContext& context) {
    state.value += static_cast<std::int64_t>(context.tick);
  });

  source.run_for_ticks(3);

  CounterSnapshotAdapter adapter;
  const auto boundary = source.capture_snapshot_boundary(adapter);

  xenor::SimulationEngine<CounterState> restored{xenor::SimulationConfig{1ms, 19}};
  restored.restore_snapshot_boundary(boundary, adapter);

  REQUIRE(adapter.capture_calls == 1);
  REQUIRE(adapter.supports_calls == 1);
  REQUIRE(adapter.restore_calls == 1);
  REQUIRE(identical(source.capture_snapshot(), restored.capture_snapshot()));
}

TEST_CASE("Malformed snapshot boundary metadata is rejected",
          "[snapshot][boundary]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms, 19}};
  engine.add_system("increment", [](CounterState& state, const xenor::StepContext&) {
    state.value += 1;
  });

  engine.run_for_ticks(2);

  CounterSnapshotAdapter adapter;
  auto boundary = engine.capture_snapshot_boundary(adapter);
  boundary.metadata.state_last_completed_tick = boundary.metadata.tick + 1;

  REQUIRE_THROWS_AS(
      xenor::restore_snapshot_from_boundary<CounterState>(boundary, adapter),
      std::invalid_argument);
}

TEST_CASE("Snapshot boundary restore rejects incompatible engine metadata",
          "[snapshot][boundary]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> source{xenor::SimulationConfig{1ms, 19}};
  source.add_system("increment", [](CounterState& state, const xenor::StepContext&) {
    state.value += 1;
  });

  source.run_for_ticks(2);

  CounterSnapshotAdapter adapter;
  const auto boundary = source.capture_snapshot_boundary(adapter);

  xenor::SimulationEngine<CounterState> incompatible{xenor::SimulationConfig{2ms, 19}};
  REQUIRE_THROWS_AS(incompatible.restore_snapshot_boundary(boundary, adapter),
                    std::invalid_argument);
  REQUIRE(adapter.supports_calls == 0);
  REQUIRE(adapter.restore_calls == 0);
}

TEST_CASE("Snapshot boundary restore rejects incompatible engine version before payload checks",
          "[snapshot][boundary][version]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> source{xenor::SimulationConfig{1ms, 19}};
  source.add_system("increment", [](CounterState& state, const xenor::StepContext&) {
    state.value += 1;
  });

  source.run_for_ticks(2);

  CounterSnapshotAdapter adapter;
  auto boundary = source.capture_snapshot_boundary(adapter);
  boundary.metadata.engine_version += 1;
  boundary.payload_version += 1;

  xenor::SimulationEngine<CounterState> restored{xenor::SimulationConfig{1ms, 19}};
  REQUIRE_THROWS_AS(restored.restore_snapshot_boundary(boundary, adapter),
                    std::invalid_argument);
  REQUIRE(adapter.supports_calls == 0);
  REQUIRE(adapter.restore_calls == 0);
}

TEST_CASE("Snapshot boundary restore supports explicit payload migration",
          "[snapshot][boundary][version]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> restored{xenor::SimulationConfig{1ms, 19}};
  MigratingCounterSnapshotAdapter adapter;

  const auto boundary = xenor::SnapshotBoundary<CounterPayload>{
      .metadata =
          xenor::SnapshotBoundaryMetadata{
              .engine_version = xenor::current_snapshot_boundary_engine_version,
              .tick = 3,
              .elapsed = 3ms,
              .seed = 19,
              .state_last_completed_tick = 3,
          },
      .payload_version = 1,
      .state_payload =
          CounterPayload{
              .value = 9,
              .execution_trace = {"legacy"},
          },
  };

  restored.restore_snapshot_boundary(boundary, adapter);

  REQUIRE(adapter.supports_calls == 1);
  REQUIRE(adapter.restore_calls == 1);
  REQUIRE(restored.state().value == 9);
  REQUIRE(restored.state().execution_trace ==
          std::vector<std::string>{"legacy", "migrated:v1"});
  REQUIRE(restored.state().last_completed_tick() == 3);
}

TEST_CASE("Snapshot boundary restore rejects unsupported payload versions",
          "[snapshot][boundary][version]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> source{xenor::SimulationConfig{1ms, 19}};
  source.add_system("increment", [](CounterState& state, const xenor::StepContext&) {
    state.value += 1;
  });

  source.run_for_ticks(2);

  CounterSnapshotAdapter adapter;
  auto boundary = source.capture_snapshot_boundary(adapter);
  boundary.payload_version += 1;

  xenor::SimulationEngine<CounterState> restored{xenor::SimulationConfig{1ms, 19}};
  REQUIRE_THROWS_AS(restored.restore_snapshot_boundary(boundary, adapter),
                    std::invalid_argument);
  REQUIRE(adapter.supports_calls == 1);
  REQUIRE(adapter.restore_calls == 0);
}

TEST_CASE("Snapshot boundary round-trip preserves continuation behavior",
          "[snapshot][boundary][replay]") {
  auto source = make_pipeline_engine();
  auto restored = make_pipeline_engine();
  auto uninterrupted = make_pipeline_engine();

  source.run_for_ticks(6);

  PipelineSnapshotAdapter adapter;
  const auto boundary = source.capture_snapshot_boundary(adapter);

  restored.restore_snapshot_boundary(boundary, adapter);

  source.run_for_ticks(4);
  restored.run_for_ticks(4);
  uninterrupted.run_for_ticks(10);

  REQUIRE(identical(source.capture_snapshot(), restored.capture_snapshot()));
  REQUIRE(identical(uninterrupted.capture_snapshot(), restored.capture_snapshot()));
}

TEST_CASE("Snapshot boundary projection does not change engine behavior",
          "[snapshot][boundary]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms, 19}};
  engine.add_system("increment", [](CounterState& state, const xenor::StepContext& context) {
    state.value += static_cast<std::int64_t>(context.tick);
  });

  engine.run_for_ticks(3);

  CounterSnapshotAdapter adapter;
  const auto before = engine.capture_snapshot();
  static_cast<void>(engine.capture_snapshot_boundary(adapter));
  const auto after = engine.capture_snapshot();

  REQUIRE(identical(before, after));
}

TEST_CASE("Snapshot boundary restore preserves engine-owned state metadata",
          "[snapshot][boundary][version]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> source{xenor::SimulationConfig{1ms, 19}};
  source.add_system("increment", [](CounterState& state, const xenor::StepContext&) {
    state.value += 1;
  });

  source.run_for_ticks(3);

  MetadataOwnershipSnapshotAdapter adapter;
  const auto boundary = source.capture_snapshot_boundary(adapter);

  xenor::SimulationEngine<CounterState> restored{xenor::SimulationConfig{1ms, 19}};
  restored.restore_snapshot_boundary(boundary, adapter);

  REQUIRE(restored.state().last_completed_tick() == boundary.metadata.state_last_completed_tick);
  REQUIRE(restored.capture_snapshot().tick == boundary.metadata.tick);
}

TEST_CASE("System execution order remains stable", "[engine]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{2ms, 3}};
  engine.add_system("first", [](CounterState& state, const xenor::StepContext& context) {
    state.execution_trace.push_back("first:" + std::to_string(context.tick));
  });
  engine.add_system("second", [](CounterState& state, const xenor::StepContext& context) {
    state.execution_trace.push_back("second:" + std::to_string(context.tick));
  });

  engine.step();
  engine.step();

  const std::vector<std::string> expected{
      "first:1", "second:1", "first:2", "second:2"};
  REQUIRE(engine.state().execution_trace == expected);
}

TEST_CASE("Systems execute in fixed phase order and registration order within each phase",
          "[engine][phase]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms, 13}};
  engine.add_system(xenor::SystemPhase::PostUpdate,
                    "post",
                    [](CounterState& state, const xenor::StepContext& context) {
    state.execution_trace.push_back("post:" + std::to_string(context.tick));
  });
  engine.add_system(xenor::SystemPhase::Update,
                    "update_a",
                    [](CounterState& state, const xenor::StepContext& context) {
    state.execution_trace.push_back("update_a:" + std::to_string(context.tick));
  });
  engine.add_system(xenor::SystemPhase::PreUpdate,
                    "pre_a",
                    [](CounterState& state, const xenor::StepContext& context) {
    state.execution_trace.push_back("pre_a:" + std::to_string(context.tick));
  });
  engine.add_system(xenor::SystemPhase::Update,
                    "update_b",
                    [](CounterState& state, const xenor::StepContext& context) {
    state.execution_trace.push_back("update_b:" + std::to_string(context.tick));
  });
  engine.add_system(xenor::SystemPhase::PreUpdate,
                    "pre_b",
                    [](CounterState& state, const xenor::StepContext& context) {
    state.execution_trace.push_back("pre_b:" + std::to_string(context.tick));
  });

  engine.step();

  const std::vector<std::string> expected{
      "pre_a:1", "pre_b:1", "update_a:1", "update_b:1", "post:1"};
  REQUIRE(engine.state().execution_trace == expected);
}

TEST_CASE("Default system registration uses the update phase", "[engine][phase]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms, 17}};
  engine.add_system(xenor::SystemPhase::PostUpdate,
                    "post",
                    [](CounterState& state, const xenor::StepContext& context) {
    state.execution_trace.push_back("post:" + std::to_string(context.tick));
  });
  engine.add_system("default", [](CounterState& state, const xenor::StepContext& context) {
    state.execution_trace.push_back("default:" + std::to_string(context.tick));
  });
  engine.add_system(xenor::SystemPhase::PreUpdate,
                    "pre",
                    [](CounterState& state, const xenor::StepContext& context) {
    state.execution_trace.push_back("pre:" + std::to_string(context.tick));
  });

  engine.step();

  const std::vector<std::string> expected{"pre:1", "default:1", "post:1"};
  REQUIRE(engine.state().execution_trace == expected);
}

TEST_CASE("Restoring a snapshot returns the engine to the exact captured state",
          "[snapshot]") {
  using namespace std::chrono_literals;

  auto engine = make_pipeline_engine();
  engine.run_for_ticks(6);

  const auto checkpoint = engine.capture_snapshot();
  REQUIRE(checkpoint.tick == 6);
  REQUIRE(checkpoint.elapsed == 150ms);

  engine.run_for_ticks(5);
  const auto advanced = engine.capture_snapshot();
  REQUIRE_FALSE(identical(checkpoint, advanced));

  engine.restore_snapshot(checkpoint);
  const auto restored = engine.capture_snapshot();

  REQUIRE(identical(restored, checkpoint));
  REQUIRE(engine.clock().current_tick() == checkpoint.tick);
  REQUIRE(engine.clock().elapsed_duration() == checkpoint.elapsed);
}

TEST_CASE("Identical no-input runs produce identical snapshots", "[determinism][replay]") {
  auto first = make_pipeline_engine();
  auto second = make_pipeline_engine();

  first.run_for_ticks(16);
  second.run_for_ticks(16);

  const auto first_snapshot = first.capture_snapshot();
  const auto second_snapshot = second.capture_snapshot();

  REQUIRE(identical(first_snapshot, second_snapshot));
}

TEST_CASE("Restoring mid-run and continuing matches uninterrupted execution",
          "[determinism][replay]") {
  auto uninterrupted = make_pipeline_engine();
  auto restored = make_pipeline_engine();

  constexpr xenor::SimulationClock::tick_type checkpoint_ticks = 7;
  constexpr xenor::SimulationClock::tick_type continuation_ticks = 11;

  uninterrupted.run_for_ticks(checkpoint_ticks + continuation_ticks);
  const auto uninterrupted_final = uninterrupted.capture_snapshot();

  restored.run_for_ticks(checkpoint_ticks);
  const auto checkpoint = restored.capture_snapshot();

  restored.run_for_ticks(continuation_ticks);
  const auto first_continuation = restored.capture_snapshot();

  restored.restore_snapshot(checkpoint);
  restored.run_for_ticks(continuation_ticks);
  const auto replayed_continuation = restored.capture_snapshot();

  REQUIRE(identical(first_continuation, replayed_continuation));
  REQUIRE(identical(uninterrupted_final, replayed_continuation));
}

TEST_CASE("Restoring the same snapshot multiple times remains consistent",
          "[snapshot][replay]") {
  auto engine = make_pipeline_engine();

  engine.run_for_ticks(5);
  const auto checkpoint = engine.capture_snapshot();

  engine.run_for_ticks(4);
  engine.restore_snapshot(checkpoint);
  const auto first_restore = engine.capture_snapshot();

  engine.run_for_ticks(3);
  engine.restore_snapshot(checkpoint);
  const auto second_restore = engine.capture_snapshot();

  REQUIRE(identical(first_restore, checkpoint));
  REQUIRE(identical(second_restore, checkpoint));
}

TEST_CASE("Incompatible elapsed duration snapshots are rejected without mutating the engine",
          "[snapshot]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms, 5}};
  engine.add_system("increment", [](CounterState& state, const xenor::StepContext&) {
    state.value += 1;
  });

  engine.run_for_ticks(2);
  const auto baseline = engine.capture_snapshot();

  auto invalid_snapshot = baseline;
  invalid_snapshot.elapsed = 3ms;

  REQUIRE_THROWS_AS(engine.restore_snapshot(invalid_snapshot), std::invalid_argument);
  REQUIRE(identical(engine.capture_snapshot(), baseline));
}

TEST_CASE("Snapshots with mismatched seeds are rejected without mutating the engine",
          "[snapshot][seed]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms, 5}};
  engine.add_system("increment", [](CounterState& state, const xenor::StepContext&) {
    state.value += 1;
  });

  engine.run_for_ticks(2);
  const auto baseline = engine.capture_snapshot();

  auto invalid_snapshot = baseline;
  invalid_snapshot.seed = 9;

  REQUIRE_THROWS_AS(engine.restore_snapshot(invalid_snapshot), std::invalid_argument);
  REQUIRE(identical(engine.capture_snapshot(), baseline));
}

TEST_CASE("Input sequences are applied to successive ticks in order",
          "[input][determinism]") {
  auto engine = make_seeded_input_engine(23);
  const auto inputs = xenor::InputSequence<WorkloadInput>{
      {{3, 1}, {5, 2}, {7, 4}}};

  engine.run_for_sequence(inputs);

  const std::vector<std::string> expected{
      "1:3:1", "2:5:2", "3:7:4"};

  REQUIRE(engine.clock().current_tick() == 3);
  REQUIRE(engine.state().last_completed_tick() == 3);
  REQUIRE(engine.state().input_trace == expected);
}

TEST_CASE("Identical seeds and identical input sequences produce identical final states",
          "[determinism][input][seed]") {
  const auto inputs = make_workload_inputs();

  auto first = make_seeded_input_engine(41);
  auto second = make_seeded_input_engine(41);

  first.run_for_sequence(inputs);
  second.run_for_sequence(inputs);

  REQUIRE(identical(first.capture_snapshot(), second.capture_snapshot()));
}

TEST_CASE("Identical runs produce identical replay traces",
          "[determinism][replay][trace]") {
  const auto inputs = make_workload_inputs();

  auto first = make_seeded_input_engine(41);
  auto second = make_seeded_input_engine(41);

  first.enable_replay_capture();
  second.enable_replay_capture();

  first.run_for_sequence(inputs);
  second.run_for_sequence(inputs);

  REQUIRE(identical(first.capture_snapshot(), second.capture_snapshot()));
  REQUIRE(first.replay_trace() == second.replay_trace());
  REQUIRE(first.replay_trace().seed == 41);
}

TEST_CASE("Different input sequences produce different replay traces where input events differ",
          "[determinism][replay][trace]") {
  auto first = make_seeded_input_engine(41);
  auto second = make_seeded_input_engine(41);

  first.enable_replay_capture();
  second.enable_replay_capture();

  first.run_for_sequence(make_workload_inputs());
  second.run_for_sequence(make_variant_workload_inputs());

  REQUIRE_FALSE(first.replay_trace() == second.replay_trace());
}

TEST_CASE("Snapshot restore events are recorded in the replay trace",
          "[snapshot][replay][trace]") {
  auto engine = make_pipeline_engine();
  engine.enable_replay_capture();

  engine.run_for_ticks(4);
  const auto checkpoint = engine.capture_snapshot();

  engine.run_for_ticks(2);
  engine.restore_snapshot(checkpoint);

  const auto& trace = engine.replay_trace();

  REQUIRE(count_events(trace, xenor::ReplayEventKind::SnapshotRestored) == 1);
  REQUIRE(trace.events.back().kind == xenor::ReplayEventKind::SnapshotRestored);
  REQUIRE(trace.events.back().tick == checkpoint.tick);
  REQUIRE(trace.events.back().elapsed == checkpoint.elapsed);
}

TEST_CASE("Phased execution markers appear in deterministic order in replay traces",
          "[engine][phase][replay][trace]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms, 21}};
  engine.add_system(xenor::SystemPhase::PostUpdate,
                    "post",
                    [](CounterState&, const xenor::StepContext&) {});
  engine.add_system(xenor::SystemPhase::Update,
                    "update",
                    [](CounterState&, const xenor::StepContext&) {});
  engine.add_system(xenor::SystemPhase::PreUpdate,
                    "pre",
                    [](CounterState&, const xenor::StepContext&) {});

  engine.enable_replay_capture();
  engine.step();

  const auto& trace = engine.replay_trace();
  std::vector<std::string> system_markers;
  for (const auto& event : trace.events) {
    if (event.kind != xenor::ReplayEventKind::SystemExecuted) {
      continue;
    }

    system_markers.push_back(
        std::to_string(static_cast<int>(event.phase)) + ":" + event.system_name);
  }

  const std::vector<std::string> expected{
      "0:pre", "1:update", "2:post"};
  REQUIRE(system_markers == expected);
}

TEST_CASE("Replay trace capture does not alter engine behavior",
          "[determinism][replay][trace]") {
  const auto inputs = make_workload_inputs();

  auto traced = make_seeded_input_engine(41);
  auto untraced = make_seeded_input_engine(41);

  traced.enable_replay_capture();

  traced.run_for_sequence(inputs);
  untraced.run_for_sequence(inputs);

  REQUIRE(identical(traced.capture_snapshot(), untraced.capture_snapshot()));
  REQUIRE_FALSE(traced.replay_trace().empty());
}

TEST_CASE("Different seeds produce different final states when seeded behavior is used",
          "[determinism][input][seed]") {
  const auto inputs = make_workload_inputs();

  auto first = make_seeded_input_engine(41);
  auto second = make_seeded_input_engine(77);

  first.run_for_sequence(inputs);
  second.run_for_sequence(inputs);

  const auto first_snapshot = first.capture_snapshot();
  const auto second_snapshot = second.capture_snapshot();

  REQUIRE_FALSE(identical(first_snapshot, second_snapshot));
  REQUIRE(first.state().random_checksum != second.state().random_checksum);
}

TEST_CASE("Different input sequences produce different final states",
          "[determinism][input]") {
  auto first = make_seeded_input_engine(41);
  auto second = make_seeded_input_engine(41);

  first.run_for_sequence(make_workload_inputs());
  second.run_for_sequence(make_variant_workload_inputs());

  REQUIRE_FALSE(identical(first.capture_snapshot(), second.capture_snapshot()));
}

TEST_CASE("Restoring mid-run and continuing with the remaining input sequence matches uninterrupted execution",
          "[determinism][input][replay]") {
  const auto inputs = make_workload_inputs();

  auto uninterrupted = make_seeded_input_engine(41);
  auto restored = make_seeded_input_engine(41);

  constexpr std::size_t checkpoint_ticks = 5;
  const auto initial_inputs = inputs.slice(0, checkpoint_ticks);
  const auto remaining_inputs = inputs.slice(checkpoint_ticks);

  uninterrupted.run_for_sequence(inputs);
  const auto uninterrupted_final = uninterrupted.capture_snapshot();

  restored.run_for_sequence(initial_inputs);
  const auto checkpoint = restored.capture_snapshot();

  restored.run_for_sequence(remaining_inputs);
  const auto first_continuation = restored.capture_snapshot();

  restored.restore_snapshot(checkpoint);
  restored.run_for_sequence(remaining_inputs);
  const auto replayed_continuation = restored.capture_snapshot();

  REQUIRE(identical(first_continuation, replayed_continuation));
  REQUIRE(identical(uninterrupted_final, replayed_continuation));
}

TEST_CASE("Repeated seeded input runs remain deterministic", "[determinism][input][seed]") {
  const auto inputs = make_workload_inputs();

  auto first = make_seeded_input_engine(91);
  auto second = make_seeded_input_engine(91);
  auto third = make_seeded_input_engine(91);

  first.run_for_sequence(inputs);
  second.run_for_sequence(inputs);
  third.run_for_sequence(inputs);

  const auto first_snapshot = first.capture_snapshot();
  const auto second_snapshot = second.capture_snapshot();
  const auto third_snapshot = third.capture_snapshot();

  REQUIRE(identical(first_snapshot, second_snapshot));
  REQUIRE(identical(second_snapshot, third_snapshot));
}
