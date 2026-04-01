include(${PROJECT_SOURCE_DIR}/cmake/Dependencies.cmake)

xenor_engine_require_catch2()

add_executable(
  xenor_engine_tests
  simulation_engine_tests.cpp)

target_link_libraries(
  xenor_engine_tests
  PRIVATE
    xenor::engine
    Catch2::Catch2WithMain)

xenor_engine_configure_warnings(xenor_engine_tests)

include(Catch)
catch_discover_tests(xenor_engine_tests)
