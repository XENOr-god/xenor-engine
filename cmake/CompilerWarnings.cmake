function(xenor_engine_configure_warnings target)
  if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    target_compile_options(
      ${target}
      PRIVATE
        -Wall
        -Wextra
        -Wpedantic
        -Wconversion
        -Wsign-conversion
        -Wshadow
        -Wformat=2
        -Wundef
        -Wold-style-cast
        -Wnon-virtual-dtor
        -Woverloaded-virtual)
  elseif(MSVC)
    target_compile_options(${target} PRIVATE /W4 /permissive-)
  endif()
endfunction()
