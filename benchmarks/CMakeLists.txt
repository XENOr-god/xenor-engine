include(${PROJECT_SOURCE_DIR}/cmake/Dependencies.cmake)

xenor_engine_require_benchmark()

add_executable(
  xenor_engine_benchmarks
  tick_throughput_benchmark.cpp)

target_link_libraries(
  xenor_engine_benchmarks
  PRIVATE
    xenor::engine
    benchmark::benchmark_main)

xenor_engine_configure_warnings(xenor_engine_benchmarks)
