# cmake/BashVersion.cmake
# --- Bash source path and version parsing ---

# Function to parse bash version from BASH_SOURCE/version.h
# Sets L_BASH_VERSION (numeric) in parent scope
# Args:
#   bash_source - path to bash source directory containing version.h
function(parse_bash_version bash_source)
    if(NOT bash_source)
        message(FATAL_ERROR "parse_bash_version: bash_source must be set")
    endif()
    if(NOT IS_DIRECTORY "${bash_source}")
        message(FATAL_ERROR "parse_bash_version: bash_source is not a directory: ${bash_source}")
    endif()

    file(READ "${bash_source}/version.h" VERSION_H_CONTENT)
    string(REGEX REPLACE ".*version ([0-9]+)[.]([0-9]+)[.]([0-9]+).*" "\\1" _BASH_MAJOR "${VERSION_H_CONTENT}")
    string(REGEX REPLACE ".*version ([0-9]+)[.]([0-9]+)[.]([0-9]+).*" "\\2" _BASH_MINOR "${VERSION_H_CONTENT}")
    string(REGEX REPLACE ".*version ([0-9]+)[.]([0-9]+)[.]([0-9]+).*" "\\3" _BASH_PATCH "${VERSION_H_CONTENT}")

    math(EXPR L_BASH_VERSION "(${_BASH_MAJOR} * 10000) + (${_BASH_MINOR} * 100) + ${_BASH_PATCH}")

    if(L_BASH_VERSION LESS 10000 OR L_BASH_VERSION GREATER 100000)
        message(FATAL_ERROR "Invalid bash version: ${L_BASH_VERSION} (expected 10000-100000, e.g., 501000 for 5.1)")
    endif()
    message(STATUS "Bash version: ${_BASH_MAJOR}.${_BASH_MINOR}.${_BASH_PATCH} (${L_BASH_VERSION})")
    set(L_BASH_VERSION ${L_BASH_VERSION} PARENT_SCOPE)
endfunction()
