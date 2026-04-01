#pragma once

#include <chrono>
#include <cstdint>

namespace xenor {

class SimulationConfig {
public:
  using duration_type = std::chrono::nanoseconds;

  explicit SimulationConfig(duration_type tick_duration);

  [[nodiscard]] duration_type tick_duration() const noexcept;
  [[nodiscard]] std::int64_t tick_duration_count() const noexcept;

private:
  duration_type tick_duration_{};
};

}  // namespace xenor
