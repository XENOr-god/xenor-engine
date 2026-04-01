#pragma once

#include <cstdint>

namespace xenor {

class SimulationState {
public:
  SimulationState() = default;
  SimulationState(const SimulationState&) = default;
  SimulationState(SimulationState&&) noexcept = default;
  SimulationState& operator=(const SimulationState&) = default;
  SimulationState& operator=(SimulationState&&) noexcept = default;
  virtual ~SimulationState() = default;

  [[nodiscard]] std::uint64_t last_completed_tick() const noexcept;
  void set_last_completed_tick(std::uint64_t tick) noexcept;

private:
  std::uint64_t last_completed_tick_{0};
};

inline std::uint64_t SimulationState::last_completed_tick() const noexcept {
  return last_completed_tick_;
}

inline void SimulationState::set_last_completed_tick(std::uint64_t tick) noexcept {
  last_completed_tick_ = tick;
}

}  // namespace xenor
