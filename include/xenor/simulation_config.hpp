#pragma once

#include <chrono>
#include <cstdint>

namespace xenor {

class SimulationConfig {
public:
  using duration_type = std::chrono::nanoseconds;
  using seed_type = std::uint64_t;

  explicit SimulationConfig(duration_type tick_duration, seed_type seed = 0);

  [[nodiscard]] duration_type tick_duration() const noexcept;
  [[nodiscard]] std::int64_t tick_duration_count() const noexcept;
  [[nodiscard]] seed_type seed() const noexcept;

private:
  duration_type tick_duration_{};
  seed_type seed_{0};
};

}  // namespace xenor
