# Generate the Rust FFI bindings with bindgen (replaces the former cargo
# build.rs). CMake knows everything bindgen needs: the bash source path, the
# resolved bash version, and ppoll availability. The output is written into a
# git-ignored directory under CMAKE_CURRENT_SOURCE_DIR so the Rust crate (which has no
# build.rs) can include it via CARGO_MANIFEST_DIR without any Cargo OUT_DIR
# plumbing.
function(generate_rust_bindings_and_version)
    set(options)
    set(one_value_args GENERATED_RUST_DIR BASH_SOURCE TARGET_NAME)
    set(multi_value_args)
    cmake_parse_arguments(ARG "${options}" "${one_value_args}" "${multi_value_args}" ${ARGN})

    # Verify required arguments
    foreach(arg GENERATED_RUST_DIR BASH_SOURCE TARGET_NAME)
        if(NOT ARG_${arg})
            message(FATAL_ERROR "generate_rust_bindings_and_version requires ${arg} argument")
        endif()
    endforeach()

    set(generated_rust_dir "${ARG_GENERATED_RUST_DIR}")
    set(bash_source "${ARG_BASH_SOURCE}")

    find_program(L_BINDGEN_EXECUTABLE NAMES bindgen)
    if(NOT L_BINDGEN_EXECUTABLE)
        message(FATAL_ERROR "bindgen not found. Install it with: cargo install bindgen-cli")
    endif()

    set(L_GENERATED_RUST_BINDINGS "${generated_rust_dir}/bash_api_gen.rs")
    add_custom_command(
        OUTPUT "${L_GENERATED_RUST_BINDINGS}"
        COMMAND "${CMAKE_COMMAND}" -E make_directory "${generated_rust_dir}"
        COMMAND "${L_BINDGEN_EXECUTABLE}"
            "${CMAKE_CURRENT_SOURCE_DIR}/l_builtin/c/bash_api.h"
            --output "${L_GENERATED_RUST_BINDINGS}"
            --allowlist-function "l_.*"
            --allowlist-function "find_variable"
            --allowlist-function "bind_variable"
            --allowlist-function "legal_identifier"
            --allowlist-function "find_function"
            --allowlist-function "make_new_array_variable"
            --allowlist-function "convert_var_to_array"
            --allowlist-function "make_new_assoc_variable"
            --allowlist-function "array_flush"
            --allowlist-function "array_insert"
            --allowlist-function "array_remove"
            --allowlist-function "assoc_flush"
            --allowlist-function "assoc_remove"
            --allowlist-function "assoc_keys_to_word_list"
            --allowlist-function "assoc_reference"
            --allowlist-function "make_word"
            --allowlist-function "make_word_list"
            --allowlist-function "execute_shell_function"
            --allowlist-function "dispose_words"
            --allowlist-function "expand_string_to_string"
            --allowlist-function "expand_string"
            --allowlist-function "builtin_usage"
            --allowlist-function "internal_getopt"
            --allowlist-function "reset_internal_getopt"
            --allowlist-function "get_name_for_error"
            --allowlist-function "executing_line_number"
            --allowlist-function "builtin_error"
            --allowlist-var "this_command_name"
            --allowlist-var "current_builtin"
            --allowlist-var "list_optarg"
            --allowlist-var "loptend"
            --allowlist-var "GETOPT_HELP"
            --allowlist-var "interactive_shell"
            --allowlist-var "ARRAY"
            --allowlist-var "l_open_flags"
            --allowlist-var "l_fd_flags"
            --allowlist-var "EX_USAGE"
            --allowlist-var "EX_RETRYFAIL"
            --allowlist-var "EX_NOTFOUND"
            --allowlist-var "EXECUTION_SUCCESS"
            --allowlist-var "EXECUTION_FAILURE"
            --allowlist-var "att_assoc"
            --allowlist-var "build_version"
            --allowlist-var "patch_level"
            --allowlist-var "dist_version"
            --allowlist-var "release_status"
            --allowlist-type "l_flag_entry_t"
            --allowlist-type "builtin"
            --opaque-type "SHELL_VAR"
            --opaque-type "ARRAY"
            --opaque-type "ARRAY_ELEMENT"
            --opaque-type "HASH_TABLE"
            --opaque-type "ARRAY_ELEMENT"
            --opaque-type "HASH_TABLE"
            --default-macro-constant-type "signed"
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
            DEPENDS
                "${CMAKE_CURRENT_SOURCE_DIR}/l_builtin/c/bash_api.h"
                "${L_BINDGEN_EXECUTABLE}"
        VERBATIM
        COMMENT "Generating Rust bindings: ${generated_rust_dir}/bash_api_gen.rs"
    )

    set(L_GENERATED_RUST_DIR "${generated_rust_dir}" PARENT_SCOPE)
    set(L_GENERATED_RUST_BINDINGS "${generated_rust_dir}/bash_api_gen.rs" PARENT_SCOPE)
    set(L_BINDGEN_EXECUTABLE "${L_BINDGEN_EXECUTABLE}" PARENT_SCOPE)

    function(write_if_changed file content)
        if(EXISTS "${file}")
            file(READ "${file}" _old_content)
        else()
            set(_old_content "")
        endif()
        if(NOT _old_content STREQUAL "${content}")
            file(WRITE "${file}" "${content}")
        endif()
    endfunction()

    # Generate version info for the version subcommand
    function(generate_version_info gen_file l_builtin_source bash_source)
        # Use shared functions to extract metadata
        get_git_commit("${l_builtin_source}" L_BUILTIN_COMMIT)
        get_cargo_version("${l_builtin_source}/l_builtin/Cargo.toml" L_BUILTIN_VERSION)
        get_git_commit("${bash_source}" BASH_COMMIT)
        # Bash version already computed as L_BASH_VERSION
        # Convert to human readable: 50116 -> 5.1.16
        math(EXPR BASH_MAJOR "${L_BASH_VERSION} / 10000")
        math(EXPR BASH_MINOR "(${L_BASH_VERSION} % 10000) / 100")
        math(EXPR BASH_PATCH "${L_BASH_VERSION} % 100")
        set(BASH_VERSION_STR "${BASH_MAJOR}.${BASH_MINOR}.${BASH_PATCH}")
        write_if_changed("${gen_file}" "
// Auto-generated by CMake. Do not edit.
pub const L_BUILTIN_VERSION: &str = \"${L_BUILTIN_VERSION}\";
pub const L_BUILTIN_COMMIT: &str = \"${L_BUILTIN_COMMIT}\";
pub const BASH_VERSION: &str = \"${BASH_VERSION_STR}\";
pub const BASH_COMMIT: &str = \"${BASH_COMMIT}\";
")
        message(STATUS "Generated version info: ${gen_file}")
    endfunction()

    generate_version_info("${generated_rust_dir}/version.rs" "${CMAKE_CURRENT_SOURCE_DIR}" "${bash_source}")
endfunction()