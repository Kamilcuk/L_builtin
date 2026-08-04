cmake_minimum_required(VERSION 3.16)

option(L_DEV "Enable development options" OFF)
if(L_DEV)
    set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
endif()

project(L_builtin C)

###############################################################################

# Bash source path (for internal headers like stdc.h, flags.h, etc.)
set(BASH_SOURCE "" CACHE PATH "Path to compiled Bash source tree")
if(NOT BASH_SOURCE)
    message(FATAL_ERROR "BASH_SOURCE must be set to the path of bash source code")
endif()
if(NOT IS_DIRECTORY "${BASH_SOURCE}")
    message(FATAL_ERROR "BASH_SOURCE is not a directory: ${BASH_SOURCE}")
endif()

# Determine bash version from BASH_SOURCE/version.h
file(READ "${BASH_SOURCE}/version.h" VERSION_H_CONTENT)
string(REGEX REPLACE ".*version ([0-9]+)[.]([0-9]+)[.]([0-9]+).*" "\\1" BASH_VERSION "${VERSION_H_CONTENT}")
math(EXPR L_BASH_VERSION "(${CMAKE_MATCH_1} * 10000) + (${CMAKE_MATCH_2} * 100) + ${CMAKE_MATCH_3}")

if(L_BASH_VERSION LESS 10000 OR L_BASH_VERSION GREATER 100000)
    message(FATAL_ERROR "Invalid bash version: ${L_BASH_VERSION} (expected 10000-100000, e.g., 501000 for 5.1)")
endif()
message(STATUS "Determined Bash version: ${L_BASH_VERSION}, using headers: ${BASH_SOURCE}")

###############################################################################

# Corrosion FetchContent
include(FetchContent)
FetchContent_Declare(
    Corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG v0.6.1
)
FetchContent_MakeAvailable(Corrosion)

# Detect optional syscalls so we can enable the matching Cargo features.
include(CheckSymbolExists)
set(CMAKE_REQUIRED_DEFINITIONS -D_GNU_SOURCE)
check_symbol_exists(ppoll "poll.h" HAVE_PPOLL)
if(HAVE_PPOLL)
    add_definitions(-DHAVE_PPOLL)
    message(STATUS "ppoll(2) found, enabling ppoll subcommand")
    set(L_BUILTIN_RS_FEATURES ppoll)
else()
    message(WARNING "ppoll(2) not found, disabling ppoll subcommand")
    set(L_BUILTIN_RS_FEATURES "")
endif()

# Enable bash_lt_4_3 feature when bash version < 4.3 (40300)
# This disables bash.call which uses l_execute_command_string on older bash
if(L_BASH_VERSION LESS 40300)
    message(STATUS "Bash version < 4.3, enabling bash_lt_4_3 feature (disables bash.call)")
    list(APPEND L_BUILTIN_RS_FEATURES bash_lt_4_3)
endif()

# Define the Rust crate for Corrosion
corrosion_import_crate(
    MANIFEST_PATH ${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml
    IMPORTED_CRATES L_BUILTIN_RS_TARGETS
    FEATURES ${L_BUILTIN_RS_FEATURES}
)

###############################################################################

add_library(L_builtin MODULE
    src/L_builtin.c
    src/lseek.c
    src/poll.c
    src/sig.c
    src/pipe.c
    src/net.c
    src/bash_api.c
)
set_target_properties(L_builtin PROPERTIES
    PREFIX ""
    SUFFIX ".so"
    OUTPUT_NAME "L_builtin"
)
target_compile_definitions(L_builtin PRIVATE HAVE_CONFIG_H SHELL L_BASH_VERSION=${L_BASH_VERSION})
target_include_directories(L_builtin PRIVATE
    ${BASH_SOURCE}
    ${BASH_SOURCE}/include
    ${BASH_SOURCE}/builtins
)
target_link_libraries(L_builtin PRIVATE pthread dl m rt ${L_BUILTIN_RS_TARGETS})
set_target_properties(L_builtin PROPERTIES C_STANDARD 99 C_EXTENSIONS ON)
add_dependencies(L_builtin ${L_BUILTIN_RS_TARGETS})

# Export only L_builtin_struct (the symbol bash dlsym()s); hide everything
# else so no symbol in this module can collide/interpose with bash's
# -rdynamic exports or other loadables.
set(VERSION_SCRIPT ${CMAKE_CURRENT_SOURCE_DIR}/src/L_builtin.map)
target_link_options(L_builtin PRIVATE -Wl,--version-script=${VERSION_SCRIPT})
set_target_properties(L_builtin PROPERTIES LINK_DEPENDS ${VERSION_SCRIPT})

###############################################################################

if(L_DEV)
    if(CMAKE_C_COMPILER_ID STREQUAL "GNU")
        target_compile_options(L_builtin PRIVATE -Wall -Wextra -fanalyzer)
    endif()

    add_custom_target(symlink_compile_commands ALL
        COMMAND ${CMAKE_COMMAND} -E create_symlink
            ${CMAKE_CURRENT_BINARY_DIR}/compile_commands.json
            ${CMAKE_CURRENT_SOURCE_DIR}/compile_commands.json
        COMMENT "Creating symlink to compile_commands.json"
    )
    add_custom_command(TARGET L_builtin POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E create_symlink
            $<TARGET_FILE:L_builtin>
            ${CMAKE_SOURCE_DIR}/L_builtin.so
        COMMENT "Creating symlink to L_builtin.so"
    )
endif()
