cmake_minimum_required(VERSION 3.24)
if(CMAKE_HOST_APPLE AND NOT DEFINED CMAKE_OSX_DEPLOYMENT_TARGET)
    set(CMAKE_OSX_DEPLOYMENT_TARGET "13.0" CACHE STRING "Minimum macOS deployment target")
endif()
project(fuckMail VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

file(READ "${CMAKE_SOURCE_DIR}/scripts/toolchain.json" FM_TOOLCHAIN_JSON)
string(JSON FM_QT_VERSION GET "${FM_TOOLCHAIN_JSON}" qt)
find_package(Qt6 ${FM_QT_VERSION} EXACT REQUIRED COMPONENTS Quick QuickControls2 Network)
qt_standard_project_setup(REQUIRES 6.8)
include(CTest)
option(FM_GUI_ONLY "Compile only the Qt client while editing UI; not an installable app" OFF)
if(FM_GUI_ONLY)
    add_subdirectory(gui)
    message(STATUS "GUI-only developer compilation: engine, packaging and integration tests are excluded")
    return()
endif()
find_program(FM_CARGO cargo REQUIRED)

set(FM_CARGO_PROFILE debug)
set(FM_CARGO_PROFILE_ARGS)
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(FM_CARGO_PROFILE release)
    set(FM_CARGO_PROFILE_ARGS --release)
endif()
set(FM_RUST_BIN_DIR "${CMAKE_SOURCE_DIR}/target/${FM_CARGO_PROFILE}")
add_custom_target(fm-rust ALL
    COMMAND "${FM_CARGO}" build --workspace --locked ${FM_CARGO_PROFILE_ARGS}
        --target-dir "${CMAKE_SOURCE_DIR}/target"
    WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
    COMMENT "Build the shared Rust engine and CLI"
    USES_TERMINAL)

add_subdirectory(gui)
add_dependencies(fuckmail-gui fm-rust)

# Give developer launches exactly the same sibling-engine discovery as a package.
add_custom_target(fm-client-runtime ALL
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
        "${FM_RUST_BIN_DIR}/fm-engine${CMAKE_EXECUTABLE_SUFFIX}"
        "${FM_RUST_BIN_DIR}/fm${CMAKE_EXECUTABLE_SUFFIX}"
        "$<TARGET_FILE_DIR:fuckmail-gui>"
    DEPENDS fuckmail-gui fm-rust)

install(TARGETS fuckmail-gui BUNDLE DESTINATION . RUNTIME DESTINATION bin)
if(APPLE)
    set(FM_INSTALL_BIN "fuckmail-gui.app/Contents/MacOS")
else()
    set(FM_INSTALL_BIN bin)
endif()
install(PROGRAMS
    "${FM_RUST_BIN_DIR}/fm-engine${CMAKE_EXECUTABLE_SUFFIX}"
    "${FM_RUST_BIN_DIR}/fm${CMAKE_EXECUTABLE_SUFFIX}"
    DESTINATION "${FM_INSTALL_BIN}")
install(FILES "${CMAKE_SOURCE_DIR}/gui/fonts/LICENSE-Geist.txt"
    DESTINATION "${FM_INSTALL_BIN}/licenses")
# The normal OS plugin remains selected by Qt. Ship offscreen too so the exact
# relocated development artifact can run its headless integration checks in CI.
if(APPLE)
    install(FILES "$<TARGET_FILE:Qt6::QOffscreenIntegrationPlugin>"
        DESTINATION "fuckmail-gui.app/Contents/PlugIns/platforms")
endif()
qt_generate_deploy_qml_app_script(
    TARGET fuckmail-gui OUTPUT_SCRIPT FM_DEPLOY_SCRIPT
    INCLUDE_PLUGINS qoffscreen
    NO_UNSUPPORTED_PLATFORM_ERROR)
install(SCRIPT "${FM_DEPLOY_SCRIPT}")

if(BUILD_TESTING)
    set(FM_GUI_TEST_ENV "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software;QT_FORCE_STDERR_LOGGING=1")
    if(WIN32)
        # Offscreen uses Qt's basic font database, not the Windows platform
        # font database. Read the host's normal fonts; do not bundle SDK fonts.
        list(APPEND FM_GUI_TEST_ENV "QT_QPA_FONTDIR=$ENV{SystemRoot}/Fonts")
    endif()
    # Optimized test binaries require their own cold compilation (including dev
    # dependencies). Do not charge that build against the execution watchdog.
    add_test(NAME rust-test-build COMMAND "${FM_CARGO}" test --workspace --locked --no-run
        ${FM_CARGO_PROFILE_ARGS} --target-dir "${CMAKE_SOURCE_DIR}/target")
    set_tests_properties(rust-test-build PROPERTIES WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
        TIMEOUT 900 FIXTURES_SETUP rust_test_binaries RESOURCE_LOCK fm_cargo)
    add_test(NAME rust-workspace COMMAND "${FM_CARGO}" test --workspace --locked
        ${FM_CARGO_PROFILE_ARGS} --target-dir "${CMAKE_SOURCE_DIR}/target")
    set_tests_properties(rust-workspace PROPERTIES WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
        TIMEOUT 180 FIXTURES_REQUIRED rust_test_binaries RESOURCE_LOCK fm_cargo)
    # --smoke exercises a disposable profile and must exit nonzero on load/IPC errors.
    add_test(NAME gui-smoke COMMAND "$<TARGET_FILE:fuckmail-gui>" --smoke)
    set_tests_properties(gui-smoke PROPERTIES TIMEOUT 45
        ENVIRONMENT "${FM_GUI_TEST_ENV}")
    add_test(NAME gui-integration COMMAND "$<TARGET_FILE:fuckmail-gui>" --self-test)
    set_tests_properties(gui-integration PROPERTIES TIMEOUT 150
        ENVIRONMENT "${FM_GUI_TEST_ENV};FM_GUI_TEST_TRACE=1")
    add_test(NAME gui-startup-recovery COMMAND "${FM_CARGO}" test -p fm-engine --test sessions
        --locked ${FM_CARGO_PROFILE_ARGS} --target-dir "${CMAKE_SOURCE_DIR}/target"
        gui_startup_recovers_after_writer_closes -- --ignored --exact --nocapture)
    set_tests_properties(gui-startup-recovery PROPERTIES WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
        TIMEOUT 90 FIXTURES_REQUIRED rust_test_binaries RESOURCE_LOCK fm_cargo
        ENVIRONMENT "${FM_GUI_TEST_ENV};FM_GUI_TEST_EXECUTABLE=$<TARGET_FILE:fuckmail-gui>")
endif()
