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

# Auto-initialize git submodules if not already initialized
function(ensure_submodule submodule_name submodule_file)
    set(submodule_path "${CMAKE_CURRENT_SOURCE_DIR}/${submodule_name}/${submodule_file}")
    if(NOT EXISTS "${submodule_path}")
        message(STATUS "File not found: ${submodule_path}. Initializing submodule: git submodule update --init ${submodule_name}")
        execute_process(
            COMMAND git submodule update --init "${submodule_name}"
            WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
            RESULT_VARIABLE result
        )
        if(result)
            message(FATAL_ERROR "Failed to initialize submodule: ${submodule_name}")
        endif()
    endif()
endfunction()

ensure_submodule(
    "third_party/corrosion"
    "CMakeLists.txt"
)
ensure_submodule(
    "third_party/boost_preprocessor"
    "CMakeLists.txt"
)

add_subdirectory(third_party/corrosion)

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

# Generate the Rust FFI bindings with bindgen (replaces the former cargo
# build.rs). CMake knows everything bindgen needs: the bash source path, the
# resolved bash version, and ppoll availability. The output is written into a
# git-ignored directory under CMAKE_CURRENT_SOURCE_DIR so the Rust crate (which has no
# build.rs) can include it via CARGO_MANIFEST_DIR without any Cargo OUT_DIR
# plumbing.
find_program(L_BINDGEN_EXECUTABLE NAMES bindgen)
if(NOT L_BINDGEN_EXECUTABLE)
    message(FATAL_ERROR "bindgen not found. Install it with: cargo install bindgen-cli")
endif()

set(L_GENERATED_RUST_DIR "${CMAKE_CURRENT_SOURCE_DIR}/generated_rust")
function(generate_bash_bindings gen_file bash_source)
    file(MAKE_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/generated_rust")
    execute_process(
        COMMAND "${L_BINDGEN_EXECUTABLE}"
            "${CMAKE_CURRENT_SOURCE_DIR}/src/bash_api.h"
            --output "${gen_file}"
            --allowlist-function l_.*
            # Bash entry points Rust calls directly (same allowlist as the
            # former build.rs), not covered by the l_.* prefix.
            --allowlist-function find_variable
            --allowlist-function bind_variable
            --allowlist-function legal_identifier
            --allowlist-function find_function
            --allowlist-function make_new_array_variable
            --allowlist-function convert_var_to_array
            --allowlist-function make_new_assoc_variable
            --allowlist-function array_flush
            --allowlist-function array_insert
            --allowlist-function array_remove
            --allowlist-function assoc_flush
            --allowlist-function assoc_remove
            --allowlist-function assoc_keys_to_word_list
            --allowlist-function assoc_reference
            --allowlist-function make_word
            --allowlist-function make_word_list
            --allowlist-function execute_shell_function
            --allowlist-function dispose_words
            --allowlist-function expand_string_to_string
            --allowlist-function expand_string
            --allowlist-function builtin_usage
            --allowlist-function internal_getopt
            --allowlist-function reset_internal_getopt
            --allowlist-function get_name_for_error
            --allowlist-function executing_line_number
            --allowlist-function builtin_error
            --allowlist-var this_command_name
            --allowlist-var current_builtin
            --allowlist-var list_optarg
            --allowlist-var loptend
            --allowlist-var GETOPT_HELP
            --allowlist-var interactive_shell
            --allowlist-var ARRAY
            --allowlist-var l_open_flags
            --allowlist-var l_fd_flags
            --allowlist-type l_flag_entry_t
            --allowlist-var EX_USAGE
            --allowlist-var EX_RETRYFAIL
            --allowlist-var EX_NOTFOUND
            --allowlist-var EXECUTION_SUCCESS
            --allowlist-var EXECUTION_FAILURE
            --allowlist-var att_assoc
            --allowlist-var build_version
            --allowlist-var patch_level
            --allowlist-var dist_version
            --allowlist-var release_status
            # --fit-macro-constant-types
            --default-macro-constant-type signed
            --allowlist-type builtin
            --opaque-type SHELL_VAR
            --opaque-type ARRAY
            --opaque-type ARRAY_ELEMENT
            --opaque-type HASH_TABLE
            --
            -DHAVE_CONFIG_H
            -DHAVE_PPOLL=1
            "-DL_BASH_VERSION=${L_BASH_VERSION}"
            -DSHELL
            -D_GNU_SOURCE=1
            -std=gnu99
            "-I${bash_source}"
            "-I${bash_source}/include"
            "-I${bash_source}/builtins"
            "-I${bash_source}/lib"
        RESULT_VARIABLE l_bindgen_result
        OUTPUT_VARIABLE l_bindgen_stdout
        ERROR_VARIABLE l_bindgen_stderr
    )
    if(NOT l_bindgen_result EQUAL 0)
        message(FATAL_ERROR
            "bindgen failed for src/bash_api.h (result ${l_bindgen_result})\n"
            "stdout: ${l_bindgen_stdout}\nstderr: ${l_bindgen_stderr}")
    endif()
endfunction()

generate_bash_bindings("${L_GENERATED_RUST_DIR}/bash_api_gen.rs" "${BASH_SOURCE}")
message(STATUS "Generated Rust bindings: ${L_GENERATED_RUST_DIR}/bash_api_gen.rs")

# Generate version info for the version subcommand
function(generate_version_info gen_file l_builtin_source bash_source)
    # Get L_builtin git commit
    execute_process(
        COMMAND git -C "${l_builtin_source}" rev-parse --short HEAD
        OUTPUT_VARIABLE L_BUILTIN_COMMIT
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET
    )
    if(NOT L_BUILTIN_COMMIT)
        set(L_BUILTIN_COMMIT "unknown")
    endif()

    # Get L_builtin version from Cargo.toml
    file(READ "${l_builtin_source}/Cargo.toml" CARGO_CONTENT)
    string(REGEX MATCH "version = \"([^\"]+)\"" _ ${CARGO_CONTENT})
    set(L_BUILTIN_VERSION ${CMAKE_MATCH_1})
    if(NOT L_BUILTIN_VERSION)
        set(L_BUILTIN_VERSION "0.0.0")
    endif()

    # Get bash git commit
    execute_process(
        COMMAND git -C "${bash_source}" rev-parse --short HEAD
        OUTPUT_VARIABLE BASH_COMMIT
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET
    )
    if(NOT BASH_COMMIT)
        set(BASH_COMMIT "unknown")
    endif()

    # Bash version already computed as L_BASH_VERSION
    # Convert to human readable: 50116 -> 5.1.16
    math(EXPR BASH_MAJOR "${L_BASH_VERSION} / 10000")
    math(EXPR BASH_MINOR "(${L_BASH_VERSION} % 10000) / 100")
    math(EXPR BASH_PATCH "${L_BASH_VERSION} % 100")
    set(BASH_VERSION_STR "${BASH_MAJOR}.${BASH_MINOR}.${BASH_PATCH}")

    file(WRITE "${gen_file}" "
// Auto-generated by CMake. Do not edit.
pub const L_BUILTIN_VERSION: &str = \"${L_BUILTIN_VERSION}\";
pub const L_BUILTIN_COMMIT: &str = \"${L_BUILTIN_COMMIT}\";
pub const BASH_VERSION: &str = \"${BASH_VERSION_STR}\";
pub const BASH_COMMIT: &str = \"${BASH_COMMIT}\";
")
    message(STATUS "Generated version info: ${gen_file}")
endfunction()

generate_version_info("${L_GENERATED_RUST_DIR}/version.rs" "${CMAKE_CURRENT_SOURCE_DIR}" "${BASH_SOURCE}")

###############################################################################

# Define the Rust crate for Corrosion
corrosion_import_crate(
    MANIFEST_PATH ${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml
    IMPORTED_CRATES L_BUILTIN_RS_TARGETS
    FEATURES ${L_BUILTIN_RS_FEATURES}
)

###############################################################################

add_library(boost_preprocessor INTERFACE)
target_include_directories(boost_preprocessor INTERFACE
    "${CMAKE_CURRENT_SOURCE_DIR}/boost_preprocessor/include"
)

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

function(assert_list_has_no_duplicates list_var msg)
    set(temp "${${list_var}}")
    list(REMOVE_DUPLICATES temp)
    list(LENGTH ${list_var} len_orig)
    list(LENGTH temp len_dedup)
    if(NOT len_orig EQUAL len_dedup)
        message(FATAL_ERROR "Assertion failed: ${msg} (found duplicates in ${list_var})")
    endif()
endfunction()

function(generate_bash_loadables_gen gen_file sources_var bash_source)
    file(GLOB sources CONFIGURE_DEPENDS SORT_BY MODIFY ORDER DESCENDING "${bash_source}/examples/loadables/*.c")
    set(loadable_names "")
    set(valid_sources "")
    set(excluded_sources "")
    foreach(src ${sources})
        get_filename_component(src_name "${src}" NAME)
        if(src_name IN_LIST excluded_sources)
            continue()
        endif()
        file(READ "${src}" src_content)
        set(regex "struct[ \t]+builtin[ \t]+([A-Za-z0-9_]+)")
        string(REGEX MATCHALL "${regex}" matched_structs "${src_content}")
        if(matched_structs)
            set(file_has_conflict FALSE)
            set(file_struct_names "")
            foreach(match ${matched_structs})
                string(REGEX REPLACE "${regex}" "\\1" struct_name "${match}")
                if(struct_name IN_LIST loadable_names)
                    set(file_has_conflict TRUE)
                    break()
                endif()
                list(APPEND file_struct_names "${struct_name}")
            endforeach()
            if(file_has_conflict)
                message(STATUS "Skipping older/duplicate source: ${src}")
            else()
                list(APPEND valid_sources "${src}")
                list(APPEND loadable_names ${file_struct_names})
            endif()
        else()
            message(WARNING "Skipping ${src}: no symbol matching '${regex}' found")
        endif()
    endforeach()
    assert_list_has_no_duplicates(loadable_names "Duplicate builtin struct registered across loadable sources")
    assert_list_has_no_duplicates(valid_sources "Duplicate builtin files found")
    list(SORT loadable_names)
    list(LENGTH loadable_names loadable_count)
    string(JOIN " " loadable_names_str ${loadable_names})
    message(STATUS "Found ${loadable_count} bash loadables: ${loadable_names_str}")
    set(c_content "/* Automatically generated by CMake. Do not edit. */\n\n#include <builtins.h>\n\n")
    foreach(name ${loadable_names})
        string(APPEND c_content "extern const struct builtin ${name};\n")
    endforeach()
    string(APPEND c_content "\nstatic const struct builtin *const bash_loadables_gen[] = {\n")
    foreach(name ${loadable_names})
        string(APPEND c_content "    &${name},\n")
    endforeach()
    string(APPEND c_content "};\n")
    file(CONFIGURE OUTPUT "${gen_file}" CONTENT "${c_content}" @ONLY)
    set(${sources_var} "${valid_sources}" PARENT_SCOPE)
endfunction()

generate_bash_loadables_gen(
    "${CMAKE_CURRENT_BINARY_DIR}/generated_c/bash_loadables_gen.h"
    bash_loadable_sources
    "${BASH_SOURCE}"
)

# Separate target for bash loadables from examples
add_library(bash_loadables OBJECT ${bash_loadable_sources})
set_target_properties(bash_loadables PROPERTIES
    POSITION_INDEPENDENT_CODE ON
)
target_compile_options(bash_loadables PRIVATE
    -ffunction-sections
    -fdata-sections
    -Wno-discarded-qualifiers
    -Wno-parentheses
    -Wno-unused-variable
    -Wno-unused-parameter
    -Wno-sign-compare
    -Wno-implicit-fallthrough
    -Wno-format-security
    -Wno-unused-but-set-variable
    -w
)
target_link_libraries(bash_loadables PRIVATE bash)
target_include_directories(bash_loadables INTERFACE "${CMAKE_CURRENT_BINARY_DIR}/generated_c")

###############################################################################

# The C MODULE target is named L_builtin_module because Corrosion already
# creates an interface target named `L_builtin` for the Rust crate. The output
# file name is kept as "L_builtin" so bash dlsym()s L_builtin_struct unchanged.
# Only the C glue sources remain in C; lseek/pipe/net/etc. are implemented in
# Rust (see src/*.rs).
add_library(L_builtin_module MODULE
    ./src/L_builtin.c
    ./src/poll.c
    ./src/sig.c
    ./src/bash_api.c
    ./src/cmd_ext.c
    ./src/cmd_fcntl.c
)
set_target_properties(L_builtin_module PROPERTIES
    PREFIX ""
    SUFFIX ".so"
    OUTPUT_NAME "L_builtin"
)
# Emit one section per function/data and garbage-collect unreferenced ones at
# link time. Safe here: the version script keeps only L_builtin_struct as a
# dynamic export (bash dlsym()s it), and every other needed symbol is
# reachable internally (Rust<->C cross-references, bash_loadables_gen[]).
target_compile_options(L_builtin_module PRIVATE -ffunction-sections -fdata-sections)
target_link_options(L_builtin_module PRIVATE -Wl,--gc-sections)
find_package(Threads REQUIRED)
target_link_libraries(L_builtin_module PRIVATE
    bash
    Threads::Threads
    ${CMAKE_DL_LIBS}
    m
    rt
    boost_preprocessor
    bash_loadables
    ${L_BUILTIN_RS_TARGETS}
)
add_dependencies(L_builtin_module ${L_BUILTIN_RS_TARGETS})

# Export only L_builtin_struct (the symbol bash dlsym()s); hide everything
# else so no symbol in this module can collide/interpose with bash's
# -rdynamic exports or other loadables.
set(VERSION_SCRIPT ${CMAKE_CURRENT_SOURCE_DIR}/src/L_builtin.map)
target_link_options(L_builtin_module PRIVATE -Wl,--version-script=${VERSION_SCRIPT})
set_target_properties(L_builtin_module PROPERTIES LINK_DEPENDS ${VERSION_SCRIPT})

###############################################################################

if(L_DEV)
    # Propagate L_DEV to the C compiler so dev-only `#ifdef L_DEV` sections in
    # the C sources (e.g. the `unittest` help entry) are compiled in.
    target_compile_definitions(L_builtin_module PRIVATE L_DEV=1)
    if(CMAKE_C_COMPILER_ID STREQUAL "GNU")
        target_compile_options(L_builtin_module PRIVATE -Wall -Wextra -fanalyzer)
        target_compile_options(bash_loadables PRIVATE -Wall -Wextra)
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
endif()
