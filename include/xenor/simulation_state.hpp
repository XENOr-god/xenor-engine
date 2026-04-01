#pragma once

#include <cstdint>

namespace xenor {

template <typename State>
class SimulationEngine;

class SimulationState {
public:
  SimulationState() = default;
  SimulationState(const SimulationState&) = default;
  SimulationState(SimulationState&&) noexcept = default;
  SimulationState& operator=(const SimulationState&) = default;
  SimulationState& operator=(SimulationState&&) noexcept = default;
  virtual ~SimulationState() = default;

  [[nodiscard]] std::uint64_t last_completed_tick() const noexcept;

private:
  template <typename State>
  friend class SimulationEngine;

  std::uint64_t last_completed_tick_{0};
};

inline std::uint64_t SimulationState::last_completed_tick() const noexcept {
  return last_completed_tick_;
}

}  // namespace xenor
