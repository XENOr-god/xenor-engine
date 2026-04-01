#include <array>
#include <chrono>
#include <cstddef>
#include <cstdint>

#include <benchmark/benchmark.h>

#include "xenor/xenor.hpp"

namespace {

struct ThroughputState final : xenor::SimulationState {
  std::array<std::uint64_t, 256> lanes{};
  std::uint64_t checksum{0};
};

auto make_engine() {
  using namespace std::chrono_literals;

  xenor::SimulationEngine<ThroughputState> engine{xenor::SimulationConfig{1us}};

  engine.add_system("accumulate", [](ThroughputState& state, const xenor::StepContext& context) {
    for (std::size_t index = 0; index < state.lanes.size(); ++index) {
      state.lanes[index] +=
          static_cast<std::uint64_t>(index) + context.tick;
    }
  });

  engine.add_system("fold", [](ThroughputState& state, const xenor::StepContext&) {
    std::uint64_t checksum = 0;
    for (const auto lane : state.lanes) {
      checksum ^= lane + 0x9e3779b97f4a7c15ULL;
    }
    state.checksum = checksum;
  });

  return engine;
}

void run_ticks(xenor::SimulationEngine<ThroughputState>& engine,
               std::uint64_t ticks) {
  engine.run_for_ticks(ticks);
}

}  // namespace

static void BM_TickThroughput(benchmark::State& state) {
  const auto ticks = static_cast<std::uint64_t>(state.range(0));

  for (auto _ : state) {
    state.PauseTiming();
    auto engine = make_engine();
    state.ResumeTiming();

    run_ticks(engine, ticks);
    benchmark::DoNotOptimize(engine.state().checksum);
  }

  state.SetItemsProcessed(
      static_cast<std::int64_t>(state.iterations()) *
      static_cast<std::int64_t>(ticks));
}

BENCHMARK(BM_TickThroughput)->Arg(1)->Arg(64)->Arg(512);
