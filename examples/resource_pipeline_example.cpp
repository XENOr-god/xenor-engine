#include <chrono>
#include <cstdint>
#include <cstdlib>
#include <iostream>

#include "xenor/xenor.hpp"

namespace {

struct ResourcePipelineState final : xenor::SimulationState {
  std::uint64_t ore{0};
  std::uint64_t ingots{0};
  std::uint64_t components{0};
  std::uint64_t finished_units{0};
};

auto make_engine() {
  using namespace std::chrono_literals;

  auto engine =
      xenor::SimulationEngine<ResourcePipelineState>{xenor::SimulationConfig{50ms}};

  engine.add_system("extraction", [](ResourcePipelineState& state, const xenor::StepContext&) {
    state.ore += 5;
  });

  engine.add_system("smelting", [](ResourcePipelineState& state, const xenor::StepContext&) {
    const auto refined_batches = state.ore / 2;
    state.ore -= refined_batches * 2;
    state.ingots += refined_batches;
  });

  engine.add_system("machining", [](ResourcePipelineState& state, const xenor::StepContext&) {
    const auto component_batches = state.ingots / 3;
    state.ingots -= component_batches * 3;
    state.components += component_batches;
  });

  engine.add_system("packing", [](ResourcePipelineState& state, const xenor::StepContext&) {
    const auto finished_batches = state.components / 2;
    state.components -= finished_batches * 2;
    state.finished_units += finished_batches;
  });

  return engine;
}

bool identical(const ResourcePipelineState& left, const ResourcePipelineState& right) {
  return left.ore == right.ore && left.ingots == right.ingots &&
         left.components == right.components &&
         left.finished_units == right.finished_units &&
         left.last_completed_tick() == right.last_completed_tick();
}

}  // namespace

int main() {
  auto first_run = make_engine();
  auto second_run = make_engine();

  constexpr std::uint64_t tick_count = 12;
  first_run.run_for_ticks(tick_count);
  second_run.run_for_ticks(tick_count);

  const auto first_snapshot = first_run.snapshot();
  const auto second_snapshot = second_run.snapshot();

  if (!identical(first_snapshot.state, second_snapshot.state)) {
    std::cerr << "deterministic replay check failed\n";
    return EXIT_FAILURE;
  }

  const auto elapsed =
      std::chrono::duration_cast<std::chrono::milliseconds>(first_snapshot.elapsed);

  std::cout << "tick: " << first_snapshot.tick << '\n';
  std::cout << "elapsed_ms: " << elapsed.count() << '\n';
  std::cout << "ore: " << first_snapshot.state.ore << '\n';
  std::cout << "ingots: " << first_snapshot.state.ingots << '\n';
  std::cout << "components: " << first_snapshot.state.components << '\n';
  std::cout << "finished_units: " << first_snapshot.state.finished_units << '\n';

  return EXIT_SUCCESS;
}
