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
include(cmake/BashVersion.cmake)
parse_bash_version("${BASH_SOURCE}")

# Initialize git submodules and set up dependencies
include(cmake/Submodules.cmake)

###############################################################################

# Detect optional syscalls so we can enable the matching Cargo features.
include(CheckSymbolExists)
set(CMAKE_REQUIRED_DEFINITIONS -D_GNU_SOURCE)
check_symbol_exists(ppoll "poll.h" HAVE_PPOLL)
if(HAVE_PPOLL)
    message(STATUS "ppoll(2) found, enabling ppoll subcommand")
    set(L_BUILTIN_RS_FEATURES ppoll)
else()
    message(WARNING "ppoll(2) not found, disabling ppoll subcommand")
    set(L_BUILTIN_RS_FEATURES "")
endif()

# Enable bash_lt_4_3 feature when bash version < 4.3 (40300)
if(L_BASH_VERSION LESS 40300)
    message(STATUS "Bash version < 4.3, enabling bash_lt_4_3 feature (disables bash.call)")
    list(APPEND L_BUILTIN_RS_FEATURES bash_lt_4_3)
endif()

# Enable the dev-only feature when L_DEV is on. This gates the `unittest`
# subcommand (and any other dev-only code) so it never ships in a release .so.
if(L_DEV)
    list(APPEND L_BUILTIN_RS_FEATURES dev)
endif()

###############################################################################

# Generate the Rust FFI bindings and version info via included cmake
include(${CMAKE_CURRENT_SOURCE_DIR}/cmake/rust_bindings_and_version.cmake)
generate_rust_bindings_and_version(
    GENERATED_RUST_DIR "${CMAKE_CURRENT_SOURCE_DIR}/l_builtin/generated_rust"
    BASH_SOURCE "${BASH_SOURCE}"
    TARGET_NAME "L_builtin"
)

###############################################################################

# Define the Rust crate for Corrosion
corrosion_import_crate(
    MANIFEST_PATH ${CMAKE_CURRENT_SOURCE_DIR}/l_builtin/Cargo.toml
    IMPORTED_CRATES L_BUILTIN_RS_TARGETS
    FEATURES ${L_BUILTIN_RS_FEATURES}
)
add_custom_target(l_generated_rust DEPENDS "${L_GENERATED_RUST_BINDINGS}")
add_dependencies(_cargo-build_l_builtin l_generated_rust)

###############################################################################

set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS ON)

add_library(bash INTERFACE)
target_include_directories(bash INTERFACE
    ${BASH_SOURCE}
    ${BASH_SOURCE}/include
    ${BASH_SOURCE}/builtins
)
target_compile_definitions(bash INTERFACE
    HAVE_CONFIG_H
    SHELL
    L_BASH_VERSION=${L_BASH_VERSION}
    _GNU_SOURCE=1
    HAVE_PPOLL=$<BOOL:${HAVE_PPOLL}>
)

###############################################################################

# Generate bash loadables via included cmake
include(${CMAKE_CURRENT_SOURCE_DIR}/cmake/bash_loadables.cmake)
create_bash_loadables_target(
    TARGET_NAME bash
    BASH_SOURCE "${BASH_SOURCE}"
    GENERATED_DIR "${CMAKE_CURRENT_BINARY_DIR}/generated_c"
)

###############################################################################

# The C MODULE target is named L_builtin_object because Corrosion already
# creates an interface target named `L_builtin` for the Rust crate. The output
# file name is kept as "L_builtin" so bash dlsym()s L_builtin_struct unchanged.
# Only the C glue sources remain in C; lseek/pipe/net/etc. are implemented in
# Rust (see l_builtin/src/*.rs).

# Regular version for direct bash use - exports L_builtin_struct
add_library(L_builtin_object OBJECT
    ./l_builtin/c/L_builtin.c
    ./l_builtin/c/poll.c
    ./l_builtin/c/sig.c
    ./l_builtin/c/bash_api.c
    ./l_builtin/c/cmd_ext.c
    ./l_builtin/c/cmd_fcntl.c
    $<TARGET_OBJECTS:bash_loadables>
)
# Emit one section per function/data and garbage-collect unreferenced ones at
# link time. Safe here: the version script keeps only L_builtin_struct as a
# dynamic export (bash dlsym()s it), and every other needed symbol is
# reachable internally (Rust<->C cross-references, bash_loadables_gen[]).
target_compile_options(L_builtin_object PUBLIC -ffunction-sections -fdata-sections)
target_link_options(L_builtin_object PUBLIC -Wl,--gc-sections)
target_include_directories(L_builtin_object PUBLIC "${CMAKE_CURRENT_BINARY_DIR}/generated_c")
find_package(Threads REQUIRED)
target_link_libraries(L_builtin_object PUBLIC
    bash
    Threads::Threads
    ${CMAKE_DL_LIBS}
    m
    rt
    boost_preprocessor
    bash_loadables
    ${L_BUILTIN_RS_TARGETS}
)
add_dependencies(L_builtin_object ${L_BUILTIN_RS_TARGETS})
set_target_properties(L_builtin_object PROPERTIES POSITION_INDEPENDENT_CODE ON)

add_library(L_builtin_module MODULE
    $<TARGET_OBJECTS:L_builtin_object>
    $<TARGET_OBJECTS:bash_loadables>
)
set_target_properties(L_builtin_module PROPERTIES
    PREFIX ""
    SUFFIX ".so"
    OUTPUT_NAME "L_builtin"
)
target_link_libraries(L_builtin_module PUBLIC L_builtin_object)
set(VERSION_SCRIPT ${CMAKE_CURRENT_SOURCE_DIR}/l_builtin/L_builtin.map)
target_link_options(L_builtin_module PRIVATE -Wl,--version-script=${VERSION_SCRIPT})
set_target_properties(L_builtin_module PROPERTIES LINK_DEPENDS ${VERSION_SCRIPT})

add_library(L_builtin_embedded MODULE
    $<TARGET_OBJECTS:L_builtin_object>
    $<TARGET_OBJECTS:bash_loadables>
)
set_target_properties(L_builtin_embedded PROPERTIES
    PREFIX ""
    SUFFIX ".so"
    OUTPUT_NAME "L_builtin_embedded"
)
target_link_libraries(L_builtin_embedded PUBLIC L_builtin_object)
set(VERSION_SCRIPT ${CMAKE_CURRENT_SOURCE_DIR}/l_builtin/L_builtin_embedded.map)
target_link_options(L_builtin_embedded PRIVATE -Wl,--version-script=${VERSION_SCRIPT})
set_target_properties(L_builtin_embedded PROPERTIES LINK_DEPENDS ${VERSION_SCRIPT})

###############################################################################

if(L_DEV)
# Propagate L_DEV to the C compiler so dev-only `#ifdef L_DEV` sections in
     # the C sources (e.g. the `unittest` help entry) are compiled in.
     target_compile_definitions(L_builtin_object PRIVATE L_DEV=1)
     if(CMAKE_C_COMPILER_ID STREQUAL "GNU")
         target_compile_options(L_builtin_object PRIVATE -Wall -Wextra -fanalyzer)
     endif()

    add_custom_target(symlink_compile_commands ALL
        COMMAND ${CMAKE_COMMAND} -E create_symlink
            ${CMAKE_CURRENT_BINARY_DIR}/compile_commands.json
            ${CMAKE_CURRENT_SOURCE_DIR}/compile_commands.json
        COMMENT "Creating symlink to compile_commands.json"
    )
    add_custom_command(TARGET L_builtin_module POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E create_symlink
            $<TARGET_FILE:L_builtin_module>
            ${CMAKE_CURRENT_SOURCE_DIR}/L_builtin.so
        COMMENT "Creating symlink to L_builtin.so"
    )
    add_custom_command(TARGET L_builtin_embedded POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E create_symlink
            $<TARGET_FILE:L_builtin_embedded>
            ${CMAKE_CURRENT_SOURCE_DIR}/L_builtin.so
        COMMENT "Creating symlink to L_builtin_embedded.so"
    )
endif()
