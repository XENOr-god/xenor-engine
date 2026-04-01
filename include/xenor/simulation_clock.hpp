#pragma once

#include <cstdint>

#include "xenor/simulation_config.hpp"

namespace xenor {

class SimulationClock {
public:
  using tick_type = std::uint64_t;
  using duration_type = SimulationConfig::duration_type;

  explicit SimulationClock(SimulationConfig config);

  [[nodiscard]] tick_type current_tick() const noexcept;
  [[nodiscard]] duration_type tick_duration() const noexcept;
  [[nodiscard]] duration_type elapsed_duration() const;
  [[nodiscard]] duration_type elapsed_duration_at(tick_type tick) const;

  void advance();
  void advance_by(tick_type steps);
  void reset() noexcept;

private:
  duration_type tick_duration_{};
  tick_type current_tick_{0};
};

}  // namespace xenor
