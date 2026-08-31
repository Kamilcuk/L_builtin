cmake_minimum_required(VERSION 3.14)
project(L_builtin_dispatcher C)

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

###############################################################################

option(L_DISPATCHER_SO_FILES "" "List of pre-built .so files for the dispatcher, e.g., /path/to/L_builtin-5.1.so;/path/to/L_builtin-5.2.so")
if(NOT L_DISPATCHER_SO_FILES)
    message(FATAL_ERROR "L_DISPATCHER_SO_FILES must be set to a list of .so files")
endif()

option(L_DEV "Create symlinks in development mode" OFF)

set(ZSTD_BUILD_STATIC ON CACHE BOOL "" FORCE)
set(ZSTD_BUILD_SHARED ON CACHE BOOL "" FORCE)
set(ZSTD_BUILD_PROGRAMS ON CACHE BOOL "" FORCE)
set(ZSTD_BUILD_CONTRIB OFF CACHE BOOL "" FORCE)
set(ZSTD_BUILD_TESTS OFF CACHE BOOL "" FORCE)
add_subdirectory(
    ../third_party/zstd
    ${CMAKE_CURRENT_BINARY_DIR}/zstd
    EXCLUDE_FROM_ALL
)

function(version_to_symbol input output)
    string(REPLACE "-" "_" _result "${input}")
    string(REPLACE "." "_" _result "${_result}")
    set(${output} "${_result}" PARENT_SCOPE)
endfunction()

set(DISPATCHER_DEFINES)
set(DISPATCHER_ELEMENTS)
set(SO_XXD_FILES)
make_directory("${CMAKE_CURRENT_BINARY_DIR}/compressed")
make_directory("${CMAKE_CURRENT_BINARY_DIR}/generated")
foreach(INPUT_SO IN LISTS L_DISPATCHER_SO_FILES)
    set(rgx "/([0-9]+[.][0-9]+)/")
    if(INPUT_SO MATCHES "${rgx}")
        set(VER_STRING "${CMAKE_MATCH_1}")
    else()
        message(FATAL_ERROR "Unexpected .so: ${INPUT_SO} has to match ${rgx}")
    endif()
    version_to_symbol("${VER_STRING}" VER_STRING_UNDERSCORE)

    set(SO_NAME "L_builtin_${VER_STRING_UNDERSCORE}.so")
    set(SO_RENAMED "${CMAKE_CURRENT_BINARY_DIR}/compressed/${SO_NAME}")
    set(cmd
       objcopy --localize-symbol=L_builtin_struct  
        "${INPUT_SO}" "${SO_RENAMED}"  &&
        readelf -Ws "${SO_RENAMED}" | grep L_builtin_struct &&
        readelf --dyn-syms "${SO_RENAMED}" | grep L_builtin_struct
    )
    add_custom_command(
        OUTPUT "${SO_RENAMED}"
        COMMAND ${cmd}
        DEPENDS "${INPUT_SO}"
        COMMENT "${cmd}"
        VERBATIM
    )

    set(SO_ZST "${CMAKE_CURRENT_BINARY_DIR}/compressed/${SO_NAME}.zst")
    set(cmd $<TARGET_FILE:zstd> -f -19 "${SO_RENAMED}" -o "${SO_ZST}")
    add_custom_command(
        OUTPUT "${SO_ZST}"
        COMMAND ${cmd}
        DEPENDS "${SO_RENAMED}"
        COMMENT "${cmd}"
        VERBATIM
    )

    set(SO_XXD "${CMAKE_CURRENT_BINARY_DIR}/generated/${SO_NAME}.zst.h")
    set(cmd xxd -i < "${SO_ZST}" > "${SO_XXD}")
    add_custom_command(
        OUTPUT "${SO_XXD}"
        COMMAND ${cmd}
        DEPENDS "${SO_ZST}"
        COMMENT "${cmd}"
        VERBATIM
    )
    if("${SO_XXD}" IN_LIST SO_XXD_FILES)
        message(FATAL_ERROR "Duplicate so version file found with version ${VER_STRING}: ${INPUT_SO} IN_LIST ${SO_XXD_FILES}")
    endif()
    list(APPEND SO_XXD_FILES "${SO_XXD}" "${SO_ZST}")

    set(NAME "l_builtin_${VER_STRING_UNDERSCORE}")
    string(APPEND DISPATCHER_DEFINES "static const unsigned char ${NAME}[] = {\n")
    string(APPEND DISPATCHER_DEFINES "#include \"${SO_NAME}.zst.h\"\n")
    string(APPEND DISPATCHER_DEFINES "};\n")
    string(APPEND DISPATCHER_ELEMENTS " { \"${VER_STRING}\", ${NAME}, sizeof(${NAME}) },\n")
endforeach()


string(APPEND DISPATCHER_DEFINES "static const struct embedded_so embedded_sos[] = {\n")
string(APPEND DISPATCHER_DEFINES "${DISPATCHER_ELEMENTS}")
string(APPEND DISPATCHER_DEFINES "};\n")

write_if_changed(
    "${CMAKE_CURRENT_BINARY_DIR}/generated/dispatcher_version_table_data.h"
    "${DISPATCHER_DEFINES}"
)

add_library(L_builtin_dispatcher MODULE dispatcher.c)
set_target_properties(L_builtin_dispatcher PROPERTIES
    PREFIX "" SUFFIX ".so" OUTPUT_NAME "L_builtin"
)
target_include_directories(L_builtin_dispatcher PRIVATE
    "${CMAKE_CURRENT_BINARY_DIR}/generated"
)
target_link_libraries(L_builtin_dispatcher PRIVATE
    libzstd_static
    ${CMAKE_DL_LIBS}
)

add_custom_target(so_xxd_files DEPENDS ${SO_XXD_FILES})
add_dependencies(L_builtin_dispatcher so_xxd_files)

if(L_DEV)
    add_custom_command(TARGET L_builtin_dispatcher POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E create_symlink
            $<TARGET_FILE:L_builtin_dispatcher>
            ${CMAKE_CURRENT_SOURCE_DIR}/../L_builtin.so
    )
endif()
