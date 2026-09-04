cmake_minimum_required(VERSION 3.10)

option(L_DEV "Enable development options" OFF)
if(L_DEV)
    set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
endif()

project(L_builtin C)

set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

###############################################################################

# BASHES is the list of bash versions to build. Each one gets its own
# standalone and embedded .so, each built by its own Corrosion target.
set(BASHES "" CACHE STRING
    "Space or semicolon separated list of bash versions to build (e.g. \"5.3 5.2\")"
)
string(REPLACE " " ";" bash_versions_list "${BASHES}")
list(REMOVE_ITEM bash_versions_list "")
if(NOT bash_versions_list)
    message(FATAL_ERROR
        "BASHES must be set to a space or seimicolon separated list of bash versions"
    )
endif()

option(CARGO_LOCKED "" OFF)

# BASH_SOURCES_DIR is the directory under which per-version bash source trees
# are expected (one subdirectory per version, e.g. ${BASH_SOURCES_DIR}/5.3).
set(BASH_SOURCES_DIR "" CACHE PATH
    "Path to the directory holding per-version bash source trees"
)
if(NOT BASH_SOURCES_DIR)
    message(FATAL_ERROR "BASH_SOURCES_DIR must be set")
endif()
if(NOT IS_DIRECTORY "${BASH_SOURCES_DIR}")
    message(FATAL_ERROR "BASH_SOURCES_DIR is not a directory: ${BASH_SOURCES_DIR}")
endif()

# Initialize git submodules and set up dependencies
include(cmake/Submodules.cmake)

# Detect optional syscalls so we can enable the matching Cargo features.
# Same detection across versions (ppoll is a kernel/libc feature, not a bash
# feature), so we do it once at top level.
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
if(L_DEV)
    list(APPEND L_BUILTIN_RS_FEATURES dev)
endif()

# Determine bash version from BASH_SOURCE/version.h
include(cmake/BashVersion.cmake)

# Per-version helper: turn "5.3" -> "5_3" for target names.
function(dot_to_underscore VER OUT)
    string(REPLACE "." "_" _dir "${VER}")
    set(${OUT} "${_dir}" PARENT_SCOPE)
endfunction()
function(underscore_to_dot VER OUT)
    string(REPLACE "_" "." _dir "${VER}")
    set(${OUT} "${_dir}" PARENT_SCOPE)
endfunction()

###############################################################################

include(${CMAKE_CURRENT_SOURCE_DIR}/cmake/create_bash_target.cmake)
include(${CMAKE_CURRENT_SOURCE_DIR}/cmake/create_bash_loadables_target.cmake)
include(${CMAKE_CURRENT_SOURCE_DIR}/cmake/rust_bindings_and_version.cmake)

set(CARGO_FLAGS)
if(CARGO_LOCKED)
    list(APPEND CARGO_FLAGS LOCKED)
endif()
corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
    IMPORTED_CRATES L_BUILTIN_RS_TARGETS
    ${CARGO_FLAGS}
)
string(REPLACE ";" " " L_BUILTIN_RS_TARGETS_STR "${L_BUILTIN_RS_TARGETS}")
message(STATUS "Imported rust target: ${L_BUILTIN_RS_TARGETS_STR}")

# Per-version corrosion import: each version is a separate cargo package
# (l_builtin_<ver> at build/rust/<ver>/Cargo.toml) so each gets its own
# Corrosion target with its own generated_rust/ output and its own
# .o / .a artifacts.

set(L_DISPATCHER_SO_FILES)
foreach(ver IN LISTS bash_versions_list)
    dot_to_underscore("${ver}" ver)
    underscore_to_dot("${ver}" ver_d)
    set(BASH_SOURCE "${BASH_SOURCES_DIR}/${ver_d}")
    if(NOT IS_DIRECTORY "${BASH_SOURCE}")
        message(FATAL_ERROR "Bash source tree for version ${ver} not found at ${BASH_SOURCE}")
    endif()

    # Parse L_BASH_VERSION (numeric) from ${BASH_SOURCE}/version.h
    parse_bash_version("${BASH_SOURCE}")

    # bash_lt_4_3 feature: only enabled when bash version < 4.3 (40300).
    set(bash_lt_4_3_feature)
    if(L_BASH_VERSION LESS 40300)
        message(STATUS "Bash version < 4.3, enabling bash_lt_4_3 feature (disables bash.call)")
        set(bash_lt_4_3_feature bash_lt_4_3)
    endif()

    set(ver_generated_rust "${CMAKE_CURRENT_BINARY_DIR}/generated_rust/${ver}")
    set(ver_generated_c "${CMAKE_CURRENT_BINARY_DIR}/generated_c/${ver}")

    # Generate the Rust FFI bindings and version info for this version.
    generate_rust_bindings_and_version(
        GENERATED_RUST_DIR "${ver_generated_rust}"
        BASH_SOURCE "${BASH_SOURCE}"
        TARGET_NAME "L_builtin_${ver}"
    )

    corrosion_set_features(l_builtin_${ver}
        ${L_BUILTIN_RS_FEATURES} ${bash_lt_4_3_feature}
    )
    corrosion_set_env_vars(l_builtin_${ver}
        GENERATED_RUST=${ver_generated_rust}
    )
    add_custom_target(l_generated_rust_${ver} DEPENDS "${L_GENERATED_RUST_BINDINGS}")
    add_dependencies(_cargo-build_l_builtin_${ver} l_generated_rust_${ver})

    # Headers + compile defs from bash's source tree for this version.
    create_bash_target(bash_${ver} "${BASH_SOURCE}")

    # bash_loadables (examples/loadables/*.c) for this version.
    create_bash_loadables_target(
        "bash_loadables_${ver}"
        "${BASH_SOURCE}"
        "${ver_generated_c}"
        "${L_BASH_VERSION}"
    )
    target_link_libraries(bash_loadables_${ver} bash_${ver})

    ###########################################################################

    # The C MODULE target is named L_builtin_object because Corrosion already
    # creates an interface target named `l_builtin_<ver>` for the Rust crate.
    # Only the C glue sources remain in C; lseek/pipe/net/etc. are implemented
    # in Rust (see l_builtin/src/*.rs).

    add_library(L_builtin_object_${ver} OBJECT
        ./l_builtin/c/L_builtin.c
        ./l_builtin/c/poll.c
        ./l_builtin/c/sig.c
        ./l_builtin/c/bash_api.c
        ./l_builtin/c/cmd_ext.c
        ./l_builtin/c/cmd_fcntl.c
        $<TARGET_OBJECTS:bash_loadables_${ver}>
    )
    # target_compile_options(L_builtin_object_${ver} PUBLIC -ffunction-sections -fdata-sections)
    # target_link_options(L_builtin_object_${ver} PUBLIC -Wl,--gc-sections)
    target_include_directories(L_builtin_object_${ver} PUBLIC "${ver_generated_c}")
    find_package(Threads REQUIRED)
    target_link_libraries(L_builtin_object_${ver} PUBLIC
        bash_${ver}
        Threads::Threads
        ${CMAKE_DL_LIBS}
        m
        rt
        boost_preprocessor
        bash_loadables_${ver}
        l_builtin_${ver}
    )
    target_compile_definitions(L_builtin_object_${ver} PUBLIC
        L_BASH_VERSION=${L_BASH_VERSION}
        HAVE_PPOLL=$<BOOL:${HAVE_PPOLL}>
    )
    target_include_directories(L_builtin_object_${ver} PUBLIC l_builtin/c)

    function(add_L_builtin_library name VERSION_SCRIPT_STD)
        # Standalone .so for direct bash use - exports L_builtin_struct.
        add_library(${name} MODULE
            $<TARGET_OBJECTS:L_builtin_object_${ver}>
            $<TARGET_OBJECTS:bash_loadables_${ver}>
        )
        target_link_libraries(${name} PUBLIC
            L_builtin_object_${ver}
            bash_loadables_${ver}
        )
        set_target_properties(${name} PROPERTIES
            PREFIX ""
            SUFFIX ".so"
            OUTPUT_NAME "${name}"
        )
        target_link_options(${name} PRIVATE -Wl,--version-script=${VERSION_SCRIPT_STD})
        set_target_properties(${name} PROPERTIES LINK_DEPENDS ${VERSION_SCRIPT_STD})
    endfunction()
    add_L_builtin_library(L_builtin_standalone_${ver}
        ${CMAKE_CURRENT_SOURCE_DIR}/l_builtin/L_builtin_standalone.map
    )
    add_L_builtin_library(L_builtin_embedded_${ver}
        ${CMAKE_CURRENT_SOURCE_DIR}/l_builtin/L_builtin_embedded.map
    )
    string(APPEND L_DISPATCHER_SO_FILES " ${CMAKE_CURRENT_BINARY_DIR}/L_builtin_embedded_${ver}.so")
    add_dependencies(cargo-build_L_builtin_dispatcher L_builtin_embedded_${ver})

    if(L_DEV)
        target_compile_definitions(L_builtin_object_${ver} PRIVATE L_DEV=1)
        if(CMAKE_C_COMPILER_ID STREQUAL "GNU")
            target_compile_options(L_builtin_object_${ver} PRIVATE -Wall -Wextra -fanalyzer)
        endif()
        add_custom_command(TARGET L_builtin_standalone_${ver} POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E create_symlink
                $<TARGET_FILE:L_builtin_standalone_${ver}>
                ${CMAKE_CURRENT_SOURCE_DIR}/L_builtin.so
            COMMENT "Symlink L_builtin.so -> $<TARGET_FILE:L_builtin_standalone_${ver}>"
        )
    endif()
endforeach()

###############################################################################

message(STATUS "L_DISPATCHER_SO_FILES=${L_DISPATCHER_SO_FILES}")
make_directory(${CMAKE_CURRENT_BINARY_DIR}/dispatcher_out_dir)
corrosion_set_env_vars(L_builtin_dispatcher
    L_DISPATCHER_SO_FILES=${L_DISPATCHER_SO_FILES}
    OUT_DIR=${CMAKE_CURRENT_BINARY_DIR}/dispatcher_out_dir
)
add_custom_target(dispatcher)
add_dependencies(dispatcher cargo-build_L_builtin_dispatcher)

###############################################################################

if(L_DEV)
    add_custom_command(TARGET dispatcher POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E create_symlink
            ${CMAKE_CURRENT_BINARY_DIR}/libL_builtin_dispatcher.so
            ${CMAKE_CURRENT_SOURCE_DIR}/L_builtin.so
        COMMENT "Symlink L_builtin.so -> libL_builtin_dispatcher.so"
    )
endif()

###############################################################################

