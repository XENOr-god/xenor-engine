#pragma once

#include <concepts>
#include <cstdint>
#include <stdexcept>
#include <utility>

#include "xenor/simulation_clock.hpp"
#include "xenor/simulation_config.hpp"
#include "xenor/simulation_snapshot.hpp"
#include "xenor/simulation_state.hpp"

namespace xenor {

using snapshot_boundary_engine_version_type = std::uint32_t;
using snapshot_boundary_payload_version_type = std::uint32_t;

inline constexpr snapshot_boundary_engine_version_type
    current_snapshot_boundary_engine_version = 1;

struct SnapshotBoundaryMetadata {
  snapshot_boundary_engine_version_type engine_version{
      current_snapshot_boundary_engine_version};
  SimulationClock::tick_type tick{0};
  SimulationConfig::duration_type elapsed{};
  SimulationConfig::seed_type seed{0};
  SimulationClock::tick_type state_last_completed_tick{0};

  bool operator==(const SnapshotBoundaryMetadata&) const = default;
};

template <typename Payload>
struct SnapshotBoundary {
  SnapshotBoundaryMetadata metadata{};
  snapshot_boundary_payload_version_type payload_version{0};
  Payload state_payload{};

  bool operator==(const SnapshotBoundary&) const = default;
};

template <typename Adapter, typename State>
concept SnapshotStateAdapter =
    std::derived_from<State, SimulationState> &&
    requires(Adapter& adapter,
             const State& state,
             const typename Adapter::payload_type& payload,
             snapshot_boundary_payload_version_type payload_version) {
      typename Adapter::payload_type;
      { adapter.payload_version() } ->
          std::convertible_to<snapshot_boundary_payload_version_type>;
      { adapter.supports_payload_version(payload_version) } -> std::convertible_to<bool>;
      { adapter.capture(state) } -> std::same_as<typename Adapter::payload_type>;
      { adapter.restore(payload, payload_version) } -> std::same_as<State>;
    };

inline void validate_snapshot_boundary_engine_version(
    snapshot_boundary_engine_version_type engine_version) {
  if (engine_version != current_snapshot_boundary_engine_version) {
    throw std::invalid_argument(
        "snapshot boundary engine version is not supported");
  }
}

template <typename Adapter>
void validate_snapshot_boundary_payload_version(
    snapshot_boundary_payload_version_type payload_version,
    Adapter& adapter) {
  if (!adapter.supports_payload_version(payload_version)) {
    throw std::invalid_argument(
        "snapshot boundary payload version is not supported by the adapter");
  }
}

inline void validate_snapshot_boundary_metadata(
    const SnapshotBoundaryMetadata& metadata) {
  validate_snapshot_boundary_engine_version(metadata.engine_version);

  if (metadata.state_last_completed_tick != metadata.tick) {
    throw std::invalid_argument(
        "snapshot boundary state metadata does not match the captured tick");
  }
}

inline void validate_snapshot_boundary_metadata(
    const SnapshotBoundaryMetadata& metadata,
    const SimulationConfig& config) {
  validate_snapshot_boundary_metadata(metadata);

  SimulationClock clock{config};
  if (metadata.elapsed != clock.elapsed_duration_at(metadata.tick)) {
    throw std::invalid_argument(
        "snapshot boundary elapsed duration does not match the engine configuration");
  }

  if (metadata.seed != config.seed()) {
    throw std::invalid_argument(
        "snapshot boundary seed does not match the engine configuration");
  }
}

template <typename State>
  requires std::derived_from<State, SimulationState>
[[nodiscard]] SnapshotBoundaryMetadata make_snapshot_boundary_metadata(
    const SimulationSnapshot<State>& snapshot) {
  return SnapshotBoundaryMetadata{
      .engine_version = current_snapshot_boundary_engine_version,
      .tick = snapshot.tick,
      .elapsed = snapshot.elapsed,
      .seed = snapshot.seed,
      .state_last_completed_tick = snapshot.state.last_completed_tick(),
  };
}

template <typename State, typename Adapter>
  requires SnapshotStateAdapter<Adapter, State>
[[nodiscard]] auto make_snapshot_boundary(const SimulationSnapshot<State>& snapshot,
                                          Adapter& adapter)
    -> SnapshotBoundary<typename Adapter::payload_type> {
  return SnapshotBoundary<typename Adapter::payload_type>{
      .metadata = make_snapshot_boundary_metadata(snapshot),
      .payload_version = static_cast<snapshot_boundary_payload_version_type>(
          adapter.payload_version()),
      .state_payload = adapter.capture(snapshot.state),
  };
}

template <typename State, typename Payload, typename Adapter>
  requires SnapshotStateAdapter<Adapter, State> &&
           std::same_as<typename Adapter::payload_type, Payload>
[[nodiscard]] SimulationSnapshot<State> restore_snapshot_from_boundary(
    const SnapshotBoundary<Payload>& boundary,
    Adapter& adapter) {
  validate_snapshot_boundary_metadata(boundary.metadata);
  validate_snapshot_boundary_payload_version(boundary.payload_version, adapter);

  auto state = adapter.restore(boundary.state_payload, boundary.payload_version);
  state.set_last_completed_tick(boundary.metadata.state_last_completed_tick);

  return SimulationSnapshot<State>{
      .tick = boundary.metadata.tick,
      .elapsed = boundary.metadata.elapsed,
      .seed = boundary.metadata.seed,
      .state = std::move(state),
  };
}

}  // namespace xenor
