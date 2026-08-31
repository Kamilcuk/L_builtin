# cmake/Submodules.cmake
# --- Initialize git submodules and set up dependencies ---

function(ensure_submodule submodule_name submodule_file)
    set(submodule_path "${CMAKE_CURRENT_SOURCE_DIR}/${submodule_name}/${submodule_file}")
    if(NOT EXISTS "${submodule_path}")
        message(STATUS "Initializing submodule: ${submodule_name}")
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

ensure_submodule("third_party/corrosion" "CMakeLists.txt")
ensure_submodule("third_party/boost_preprocessor" "CMakeLists.txt")

add_subdirectory(third_party/corrosion)

# boost_preprocessor headers
add_library(boost_preprocessor INTERFACE)
target_include_directories(boost_preprocessor INTERFACE
    "${CMAKE_CURRENT_SOURCE_DIR}/boost_preprocessor/include"
)

# Function to extract git commit hash from a source directory
# Args:
#   source_dir - path to git repository
#   out_var - variable name to store the result
function(get_git_commit source_dir out_var)
    set(_commit "")
    execute_process(
        COMMAND git -C "${source_dir}" rev-parse --short HEAD
        OUTPUT_VARIABLE _commit
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET
    )
    if(NOT _commit)
        set(_commit "unknown")
    endif()
    set(${out_var} "${_commit}" PARENT_SCOPE)
endfunction()

# Function to extract version from Cargo.toml
# Args:
#   cargo_toml_path - path to Cargo.toml
#   out_var - variable name to store the result
function(get_cargo_version cargo_toml_path out_var)
    file(READ "${cargo_toml_path}" _CARGO_CONTENT)
    string(REGEX MATCH "version = \"([^\"]+)\"" _match "${_CARGO_CONTENT}")
    set(_version "${CMAKE_MATCH_1}")
    if(NOT _version)
        set(_version "0.0.0")
    endif()
    set(${out_var} "${_version}" PARENT_SCOPE)
endfunction()
