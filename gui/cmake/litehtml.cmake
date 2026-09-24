# Static builds of the vendored litehtml renderer and its bundled gumbo parser.
# See third_party/litehtml/VENDORED.md; upstream install/test targets are omitted.
enable_language(C)
set(FM_LITEHTML_DIR "${CMAKE_CURRENT_LIST_DIR}/../third_party/litehtml")

file(GLOB FM_GUMBO_SOURCES CONFIGURE_DEPENDS "${FM_LITEHTML_DIR}/src/gumbo/*.c")
add_library(fm-gumbo STATIC ${FM_GUMBO_SOURCES})
set_target_properties(fm-gumbo PROPERTIES C_STANDARD 99 POSITION_INDEPENDENT_CODE ON)
target_include_directories(fm-gumbo PUBLIC "${FM_LITEHTML_DIR}/src/gumbo/include"
    PRIVATE "${FM_LITEHTML_DIR}/src/gumbo/include/gumbo")
if(MSVC)
    target_include_directories(fm-gumbo PRIVATE "${FM_LITEHTML_DIR}/src/gumbo/visualc/include")
    target_compile_options(fm-gumbo PRIVATE /wd4244 /wd4267 /utf-8)
else()
    target_compile_options(fm-gumbo PRIVATE -w)
endif()

file(GLOB FM_LITEHTML_SOURCES CONFIGURE_DEPENDS "${FM_LITEHTML_DIR}/src/*.cpp")
add_library(fm-litehtml STATIC ${FM_LITEHTML_SOURCES})
set_target_properties(fm-litehtml PROPERTIES CXX_STANDARD 17 CXX_STANDARD_REQUIRED ON POSITION_INDEPENDENT_CODE ON)
target_include_directories(fm-litehtml PUBLIC "${FM_LITEHTML_DIR}/include"
    PRIVATE "${FM_LITEHTML_DIR}/src" "${FM_LITEHTML_DIR}/include/litehtml")
target_link_libraries(fm-litehtml PUBLIC fm-gumbo)
if(MSVC)
    target_compile_options(fm-litehtml PRIVATE /permissive- /utf-8 /W1)
else()
    target_compile_options(fm-litehtml PRIVATE -w)
endif()
