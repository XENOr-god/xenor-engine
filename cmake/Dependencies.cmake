include(FetchContent)

function(xenor_engine_require_catch2)
  if(TARGET Catch2::Catch2WithMain)
    return()
  endif()

  set(CATCH_INSTALL_DOCS OFF CACHE BOOL "" FORCE)
  set(CATCH_INSTALL_EXTRAS ON CACHE BOOL "" FORCE)
  set(CATCH_DEVELOPMENT_BUILD OFF CACHE BOOL "" FORCE)

  FetchContent_Declare(
    Catch2
    GIT_REPOSITORY https://github.com/catchorg/Catch2.git
    GIT_TAG v3.11.0
    GIT_SHALLOW TRUE)

  FetchContent_MakeAvailable(Catch2)
endfunction()

function(xenor_engine_require_benchmark)
  if(TARGET benchmark::benchmark_main)
    return()
  endif()

  set(BENCHMARK_ENABLE_TESTING OFF CACHE BOOL "" FORCE)
  set(BENCHMARK_ENABLE_GTEST_TESTS OFF CACHE BOOL "" FORCE)
  set(BENCHMARK_ENABLE_INSTALL OFF CACHE BOOL "" FORCE)
  set(BENCHMARK_DOWNLOAD_DEPENDENCIES OFF CACHE BOOL "" FORCE)

  FetchContent_Declare(
    benchmark
    GIT_REPOSITORY https://github.com/google/benchmark.git
    GIT_TAG v1.9.4
    GIT_SHALLOW TRUE)

  FetchContent_MakeAvailable(benchmark)
endfunction()
