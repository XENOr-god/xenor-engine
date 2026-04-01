#pragma once

#include <concepts>
#include <stdexcept>
#include <utility>

#include "xenor/simulation_clock.hpp"
#include "xenor/simulation_config.hpp"
#include "xenor/simulation_snapshot.hpp"
#include "xenor/simulation_state.hpp"

namespace xenor {

struct SnapshotBoundaryMetadata {
  SimulationClock::tick_type tick{0};
  SimulationConfig::duration_type elapsed{};
  SimulationConfig::seed_type seed{0};
  SimulationClock::tick_type state_last_completed_tick{0};

  bool operator==(const SnapshotBoundaryMetadata&) const = default;
};

template <typename Payload>
struct SnapshotBoundary {
  SnapshotBoundaryMetadata metadata{};
  Payload state_payload{};

  bool operator==(const SnapshotBoundary&) const = default;
};

template <typename Adapter, typename State>
concept SnapshotStateAdapter =
    std::derived_from<State, SimulationState> &&
    requires(Adapter& adapter,
             const State& state,
             const typename Adapter::payload_type& payload) {
      typename Adapter::payload_type;
      { adapter.capture(state) } -> std::same_as<typename Adapter::payload_type>;
      { adapter.restore(payload) } -> std::same_as<State>;
    };

inline void validate_snapshot_boundary_metadata(
    const SnapshotBoundaryMetadata& metadata) {
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

  auto state = adapter.restore(boundary.state_payload);
  state.set_last_completed_tick(boundary.metadata.state_last_completed_tick);

  return SimulationSnapshot<State>{
      .tick = boundary.metadata.tick,
      .elapsed = boundary.metadata.elapsed,
      .seed = boundary.metadata.seed,
      .state = std::move(state),
  };
}

}  // namespace xenor
