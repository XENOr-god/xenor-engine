#pragma once

#include <array>
#include <cstdint>

namespace xenor {

enum class SystemPhase : std::uint8_t {
  PreUpdate = 0,
  Update = 1,
  PostUpdate = 2,
};

[[nodiscard]] constexpr std::array<SystemPhase, 3> ordered_system_phases() noexcept {
  return {SystemPhase::PreUpdate, SystemPhase::Update, SystemPhase::PostUpdate};
}

}  // namespace xenor
