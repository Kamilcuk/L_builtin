cmake_minimum_required(VERSION 3.14)
project(L_builtin_dispatcher C)

option(L_DISPATCHER_SO_FILES "" "List of pre-built .so files for the dispatcher, e.g., /path/to/L_builtin-5.1.so;/path/to/L_builtin-5.2.so")
file(GLOB L_DISPATCHER_SO_FILES "${L_DISPATCHER_SO_FILES}")
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

set(DISPATCHER_OBJS)
set(DISPATCHER_VERSIONS)
foreach(VER_SO IN LISTS L_DISPATCHER_SO_FILES)
    get_filename_component(VER_SO_NAME "${VER_SO}" NAME)
    if(VER_SO_NAME MATCHES "^L_builtin-(.+)\\.so$")
        set(VER_STRING "${CMAKE_MATCH_1}")
    else()
        message(FATAL_ERROR "Unexpected .so file name: ${VER_SO_NAME}. Expected format: L_builtin-<version>.so")
    endif()
    file(SIZE "${VER_SO}" UNCOMPRESSED_SIZE)
    string(REPLACE "-" "_" VER_STRING_UNDERSCORE ${VER_STRING})
    string(REPLACE "." "_" VER_STRING_UNDERSCORE ${VER_STRING_UNDERSCORE})
    set(OBJCOPY_INPUT_BASE "L_builtin_${VER_STRING_UNDERSCORE}_so_zstd")
    set(VER_SO_LOCAL_NOEXT "${CMAKE_CURRENT_BINARY_DIR}/${OBJCOPY_INPUT_BASE}")
    set(VER_OBJ_OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/${OBJCOPY_INPUT_BASE}.o")
    set(cmd $<TARGET_FILE:zstd> -f -19 "${VER_SO}" -o "${VER_SO_LOCAL_NOEXT}")
    add_custom_command(
        OUTPUT ${VER_SO_LOCAL_NOEXT}
        COMMAND ${cmd}
        DEPENDS "${VER_SO}" $<TARGET_FILE:zstd>
        COMMENT "${cmd}"
    )
    set(cmd ${CMAKE_OBJCOPY} -I binary -O elf64-x86-64 -B i386:x86-64
                --prefix-symbols=_binary_
                ${VER_SO_LOCAL_NOEXT}
                ${VER_OBJ_OUTPUT})
    add_custom_command(
        OUTPUT ${VER_OBJ_OUTPUT}
        COMMAND ${cmd}
        DEPENDS ${VER_SO_LOCAL_NOEXT}
        COMMENT "${cmd}"
    )
    list(APPEND DISPATCHER_OBJS ${VER_OBJ_OUTPUT})
    list(APPEND DISPATCHER_VERSIONS ${VER_STRING_UNDERSCORE})
endforeach()

list(SORT DISPATCHER_VERSIONS)

# Generate the version table data.
set(DISPATCHER_VERSION_TABLE_DATA_H "${CMAKE_CURRENT_BINARY_DIR}/dispatcher_version_table_data.h")
file(WRITE "${DISPATCHER_VERSION_TABLE_DATA_H}" "")
foreach(VER IN LISTS DISPATCHER_VERSIONS)
    set(SYM _binary_L_builtin_${VER}_so_zstd)
    file(APPEND "${DISPATCHER_VERSION_TABLE_DATA_H}" "extern const unsigned char ${SYM}_start, ${SYM}_end;\n")
endforeach()
file(APPEND "${DISPATCHER_VERSION_TABLE_DATA_H}" "static struct embedded_so embedded_sos[] = {\n")
foreach(VER IN LISTS DISPATCHER_VERSIONS)
    set(SYM _binary_L_builtin_${VER}_so_zstd)
    file(APPEND "${DISPATCHER_VERSION_TABLE_DATA_H}" " { \"${VER_STRING}\", &${SYM}_start, &${SYM}_end },\n")
endforeach()
file(APPEND "${DISPATCHER_VERSION_TABLE_DATA_H}" "};\n")

add_library(L_builtin_dispatcher MODULE
    dispatcher.c
    ${DISPATCHER_OBJS}
)
set_target_properties(L_builtin_dispatcher PROPERTIES
    PREFIX "" SUFFIX ".so" OUTPUT_NAME "L_builtin"
)
target_include_directories(L_builtin_dispatcher PRIVATE ${CMAKE_CURRENT_BINARY_DIR})
target_link_libraries(L_builtin_dispatcher PRIVATE
    libzstd_static
    ${CMAKE_DL_LIBS}
)

if(L_DEV)
    add_custom_command(TARGET L_builtin_dispatcher POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E create_symlink
            $<TARGET_FILE:L_builtin_dispatcher>
            ${CMAKE_CURRENT_SOURCE_DIR}/../L_builtin.so
    )
endif()
