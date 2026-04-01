#pragma once

#include <concepts>
#include <cstdint>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "xenor/step_context.hpp"
#include "xenor/system_phase.hpp"

namespace xenor {

enum class ReplayEventKind : std::uint8_t {
  TickStarted = 0,
  InputApplied = 1,
  SystemExecuted = 2,
  TickCompleted = 3,
  SnapshotRestored = 4,
};

template <typename Input = NoInput>
struct ReplayEvent {
  ReplayEventKind kind{ReplayEventKind::TickStarted};
  SimulationClock::tick_type tick{0};
  SimulationConfig::duration_type elapsed{};
  SimulationConfig::seed_type step_seed{0};
  SystemPhase phase{SystemPhase::Update};
  std::string system_name{};
  std::optional<Input> input{};

  [[nodiscard]] static ReplayEvent tick_started(SimulationClock::tick_type tick,
                                                SimulationConfig::duration_type elapsed,
                                                SimulationConfig::seed_type step_seed) {
    return ReplayEvent{
        .kind = ReplayEventKind::TickStarted,
        .tick = tick,
        .elapsed = elapsed,
        .step_seed = step_seed,
    };
  }

  [[nodiscard]] static ReplayEvent tick_completed(SimulationClock::tick_type tick,
                                                  SimulationConfig::duration_type elapsed) {
    return ReplayEvent{
        .kind = ReplayEventKind::TickCompleted,
        .tick = tick,
        .elapsed = elapsed,
    };
  }

  [[nodiscard]] static ReplayEvent snapshot_restored(
      SimulationClock::tick_type tick,
      SimulationConfig::duration_type elapsed) {
    return ReplayEvent{
        .kind = ReplayEventKind::SnapshotRestored,
        .tick = tick,
        .elapsed = elapsed,
    };
  }

  [[nodiscard]] static ReplayEvent system_executed(
      SimulationClock::tick_type tick,
      SimulationConfig::duration_type elapsed,
      SystemPhase phase,
      std::string system_name) {
    return ReplayEvent{
        .kind = ReplayEventKind::SystemExecuted,
        .tick = tick,
        .elapsed = elapsed,
        .phase = phase,
        .system_name = std::move(system_name),
    };
  }

  [[nodiscard]] static ReplayEvent input_applied(
      SimulationClock::tick_type tick,
      SimulationConfig::duration_type elapsed,
      const Input& input_value) requires (!std::same_as<Input, NoInput>) {
    return ReplayEvent{
        .kind = ReplayEventKind::InputApplied,
        .tick = tick,
        .elapsed = elapsed,
        .input = input_value,
    };
  }

  bool operator==(const ReplayEvent&) const = default;
};

template <typename Input = NoInput>
struct ReplayTrace {
  using event_type = ReplayEvent<Input>;

  SimulationConfig::seed_type seed{0};
  std::vector<event_type> events;

  void clear() noexcept { events.clear(); }
  [[nodiscard]] bool empty() const noexcept { return events.empty(); }

  bool operator==(const ReplayTrace&) const = default;
};

}  // namespace xenor
