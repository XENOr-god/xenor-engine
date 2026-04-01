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

template <typename State>
bool identical(const xenor::SimulationSnapshot<State>& left,
               const xenor::SimulationSnapshot<State>& right) {
  return left.tick == right.tick && left.elapsed == right.elapsed &&
         identical(left.state, right.state);
}

auto make_pipeline_engine() {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<PipelineState> engine{xenor::SimulationConfig{25ms}};

  engine.add_system("mining", [](PipelineState& state, const xenor::StepContext&) {
    state.ore += 4;
  });

  engine.add_system("smelting", [](PipelineState& state, const xenor::StepContext&) {
    const auto ingot_batches = state.ore / 2;
    state.ore -= ingot_batches * 2;
    state.ingots += ingot_batches;
  });

  engine.add_system("packing", [](PipelineState& state, const xenor::StepContext&) {
    const auto finished_batches = state.ingots / 3;
    state.ingots -= finished_batches * 3;
    state.finished_units += finished_batches;
  });

  return engine;
}

}  // namespace

TEST_CASE("SimulationClock advances in fixed increments", "[clock]") {
  using namespace std::chrono_literals;

  xenor::SimulationClock clock{xenor::SimulationConfig{10ms}};

  REQUIRE(clock.current_tick() == 0);
  REQUIRE(clock.elapsed_duration() == 0ms);

  clock.advance();
  REQUIRE(clock.current_tick() == 1);
  REQUIRE(clock.elapsed_duration() == 10ms);

  clock.advance_by(4);
  REQUIRE(clock.current_tick() == 5);
  REQUIRE(clock.elapsed_duration() == 50ms);
}

TEST_CASE("SimulationEngine advances tick metadata and captures snapshots",
          "[engine][snapshot]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms}};
  engine.add_system("increment", [](CounterState& state, const xenor::StepContext& context) {
    state.value += static_cast<std::int64_t>(context.tick);
  });

  engine.run_for_ticks(3);

  REQUIRE(engine.clock().current_tick() == 3);
  REQUIRE(engine.state().last_completed_tick() == 3);
  REQUIRE(engine.state().value == 6);

  const auto snapshot = engine.capture_snapshot();
  const auto compatibility_snapshot = engine.snapshot();

  REQUIRE(snapshot.tick == 3);
  REQUIRE(snapshot.elapsed == 3ms);
  REQUIRE(snapshot.state.value == 6);
  REQUIRE(snapshot.state.last_completed_tick() == 3);
  REQUIRE(identical(snapshot, compatibility_snapshot));
}

TEST_CASE("System execution order remains stable", "[engine]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{2ms}};
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

TEST_CASE("Identical runs produce identical snapshots", "[determinism][replay]") {
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

TEST_CASE("Incompatible snapshots are rejected without mutating the engine", "[snapshot]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms}};
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
