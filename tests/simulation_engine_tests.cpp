#include <chrono>
#include <cstdint>
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

TEST_CASE("SimulationEngine advances tick metadata and state", "[engine]") {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<CounterState> engine{xenor::SimulationConfig{1ms}};
  engine.add_system("increment", [](CounterState& state, const xenor::StepContext& context) {
    state.value += static_cast<std::int64_t>(context.tick);
  });

  engine.run_for_ticks(3);

  REQUIRE(engine.clock().current_tick() == 3);
  REQUIRE(engine.state().last_completed_tick() == 3);
  REQUIRE(engine.state().value == 6);

  const auto snapshot = engine.snapshot();
  REQUIRE(snapshot.tick == 3);
  REQUIRE(snapshot.elapsed == 3ms);
  REQUIRE(snapshot.state.value == 6);
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

TEST_CASE("Identical runs produce identical snapshots", "[determinism]") {
  auto first = make_pipeline_engine();
  auto second = make_pipeline_engine();

  first.run_for_ticks(16);
  second.run_for_ticks(16);

  const auto first_snapshot = first.snapshot();
  const auto second_snapshot = second.snapshot();

  REQUIRE(first_snapshot.tick == second_snapshot.tick);
  REQUIRE(first_snapshot.elapsed == second_snapshot.elapsed);
  REQUIRE(first_snapshot.state.ore == second_snapshot.state.ore);
  REQUIRE(first_snapshot.state.ingots == second_snapshot.state.ingots);
  REQUIRE(first_snapshot.state.finished_units == second_snapshot.state.finished_units);
  REQUIRE(first_snapshot.state.last_completed_tick() ==
          second_snapshot.state.last_completed_tick());
}
