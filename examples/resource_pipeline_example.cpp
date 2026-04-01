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

bool identical(const xenor::SimulationSnapshot<ResourcePipelineState>& left,
               const xenor::SimulationSnapshot<ResourcePipelineState>& right) {
  return left.tick == right.tick && left.elapsed == right.elapsed &&
         identical(left.state, right.state);
}

}  // namespace

int main() {
  auto engine = make_engine();

  constexpr std::uint64_t checkpoint_ticks = 8;
  constexpr std::uint64_t continuation_ticks = 4;

  engine.run_for_ticks(checkpoint_ticks);
  const auto checkpoint = engine.capture_snapshot();

  engine.run_for_ticks(continuation_ticks);
  const auto uninterrupted = engine.capture_snapshot();

  engine.restore_snapshot(checkpoint);
  engine.run_for_ticks(continuation_ticks);
  const auto replayed = engine.capture_snapshot();

  if (!identical(uninterrupted, replayed)) {
    std::cerr << "deterministic replay check failed\n";
    return EXIT_FAILURE;
  }

  const auto checkpoint_elapsed =
      std::chrono::duration_cast<std::chrono::milliseconds>(checkpoint.elapsed);
  const auto elapsed =
      std::chrono::duration_cast<std::chrono::milliseconds>(uninterrupted.elapsed);

  std::cout << "checkpoint_tick: " << checkpoint.tick << '\n';
  std::cout << "checkpoint_elapsed_ms: " << checkpoint_elapsed.count() << '\n';
  std::cout << "tick: " << uninterrupted.tick << '\n';
  std::cout << "elapsed_ms: " << elapsed.count() << '\n';
  std::cout << "ore: " << uninterrupted.state.ore << '\n';
  std::cout << "ingots: " << uninterrupted.state.ingots << '\n';
  std::cout << "components: " << uninterrupted.state.components << '\n';
  std::cout << "finished_units: " << uninterrupted.state.finished_units << '\n';

  return EXIT_SUCCESS;
}
