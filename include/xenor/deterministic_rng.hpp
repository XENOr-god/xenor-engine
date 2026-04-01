#pragma once

#include <cstdint>

namespace xenor {

class DeterministicRng {
public:
  using result_type = std::uint64_t;

  explicit constexpr DeterministicRng(result_type seed = 0) noexcept : state_(seed) {}

  [[nodiscard]] constexpr result_type state() const noexcept { return state_; }

  constexpr void reseed(result_type seed) noexcept { state_ = seed; }

  [[nodiscard]] constexpr result_type next_u64() noexcept {
    state_ += 0x9e3779b97f4a7c15ULL;
    return mix(state_);
  }

  [[nodiscard]] static constexpr result_type mix(result_type value) noexcept {
    value ^= value >> 30U;
    value *= 0xbf58476d1ce4e5b9ULL;
    value ^= value >> 27U;
    value *= 0x94d049bb133111ebULL;
    value ^= value >> 31U;
    return value;
  }

private:
  result_type state_{0};
};

}  // namespace xenor
