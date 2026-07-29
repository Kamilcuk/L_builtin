cmake_minimum_required(VERSION 3.16)

option(L_DEV "Enable development options" OFF)
if(L_DEV)
    set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
endif()

project(L_builtin C)

add_compile_options(-std=gnu99)

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

# Define the Rust crate for Corrosion
corrosion_import_crate(
    MANIFEST_PATH ${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml
    IMPORTED_CRATES L_BUILTIN_RS_TARGETS
    FEATURES ${L_BUILTIN_RS_FEATURES}
)

set(BASH_INC "/usr/include/bash" CACHE PATH "Path to bash headers")

if(EXISTS "${BASH_INC}/version.h")
    file(READ "${BASH_INC}/version.h" VERSION_H_CONTENT)
    string(REGEX MATCH "#define DISTVERSION \"([0-9]+\\.[0-9]+)\"" _ "${VERSION_H_CONTENT}")
    set(BASH_VER "${CMAKE_MATCH_1}")
endif()

if(NOT BASH_VER)
    execute_process(
        COMMAND bash -c "echo -n \$BASH_VERSION"
        OUTPUT_VARIABLE RAW_BASH_VER
        OUTPUT_STRIP_TRAILING_WHITESPACE
    )
    string(REGEX MATCH "^([0-9]+\\.[0-9]+)" _ "${RAW_BASH_VER}")
    set(BASH_VER "${CMAKE_MATCH_1}")
endif()

message(STATUS "Determined Bash version: ${BASH_VER}")

string(REGEX MATCH "^([0-9]+)" _ "${BASH_VER}")
set(BASH_MAJOR_VERSION "${CMAKE_MATCH_1}")
add_definitions(-DBASH_MAJOR_VERSION=${BASH_MAJOR_VERSION})

if(NOT BASH_VER STREQUAL "3.2" AND NOT BASH_VER STREQUAL "5.2" AND NOT BASH_VER STREQUAL "5.3")
    message(WARNING "
#####################################################################
# WARNING: Bash version ${BASH_VER} is UNTESTED!
# This project is officially tested and optimized only for:
# - Bash 3.2
# - Bash 5.3
# Use at your own risk.
#####################################################################
")
endif()

set(BASH_SRC "${CMAKE_CURRENT_SOURCE_DIR}/../tmp/bash" CACHE PATH "Path to bash source")

message(STATUS "Using bash includes from: ${BASH_INC}")
message(STATUS "Using bash fallback source from: ${BASH_SRC}")

set(C_SOURCES
    src/L_builtin.c
    src/lseek.c
    src/poll.c
    src/sig.c
    src/pipe.c
    src/net.c
    src/bash_api.c
)

set(INC_DIRS
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}/src
    ${BASH_INC}
    ${BASH_INC}/include
    ${BASH_INC}/builtins
    ${BASH_SRC}
    ${BASH_SRC}/include
    ${BASH_SRC}/builtins
)

set(LIBS
    pthread
    dl
    m
    rt
)

add_library(L_builtin MODULE ${C_SOURCES})
set_target_properties(L_builtin PROPERTIES
    PREFIX ""
    SUFFIX ".so"
    OUTPUT_NAME "L_builtin"
)
target_compile_definitions(L_builtin PRIVATE HAVE_CONFIG_H SHELL)
target_include_directories(L_builtin PRIVATE ${INC_DIRS})
target_link_libraries(L_builtin PRIVATE ${LIBS} ${L_BUILTIN_RS_TARGETS})

# Export only L_builtin_struct (the symbol bash dlsym()s); hide everything
# else so no symbol in this module can collide/interpose with bash's
# -rdynamic exports or other loadables.
set(VERSION_SCRIPT ${CMAKE_CURRENT_SOURCE_DIR}/src/L_builtin.map)
target_link_options(L_builtin PRIVATE -Wl,--version-script=${VERSION_SCRIPT})
set_target_properties(L_builtin PROPERTIES LINK_DEPENDS ${VERSION_SCRIPT})

add_dependencies(L_builtin ${L_BUILTIN_RS_TARGETS})

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
endif()