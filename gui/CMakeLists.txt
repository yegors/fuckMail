qt_add_executable(fuckmail-gui WIN32 MACOSX_BUNDLE
    main.cpp
    MailBackend.cpp
    MailBackend.h
    Phase1Backend.cpp
    CalendarOperations.cpp
    ReplyDocument.cpp
    ReplyDocument.h
    CalendarLayout.cpp
    CalendarLayout.h
    GuiSelfTest.cpp
    GuiSelfTest.h
    GuiCalendarSelfTest.cpp
    GuiCalendarSelfTest.h
    GuiSearchBenchmark.cpp
    GuiSearchBenchmark.h
    IpcTrust.cpp
    IpcTrust.h
)
set_source_files_properties(Theme.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)
qt_add_qml_module(fuckmail-gui
    URI FuckMail
    VERSION 1.0
    QML_FILES Main.qml Theme.qml FmButton.qml FmToolTip.qml FmIcon.qml FmTextField.qml FmComboBox.qml MailView.qml MailFilters.qml ReplyComposer.qml CalendarView.qml EventDetails.qml EventEditor.qml CalendarOperations.qml CalendarEventList.qml SettingsView.qml TodayStrip.qml
    RESOURCES fonts/Geist-Regular.ttf fonts/Geist-Medium.ttf fonts/Geist-SemiBold.ttf fonts/LICENSE-Geist.txt
)
target_link_libraries(fuckmail-gui PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
target_compile_features(fuckmail-gui PRIVATE cxx_std_20)
if(WIN32)
    target_link_libraries(fuckmail-gui PRIVATE advapi32)
endif()
set_target_properties(fuckmail-gui PROPERTIES
    MACOSX_BUNDLE_BUNDLE_NAME "fuckMail"
    MACOSX_BUNDLE_GUI_IDENTIFIER "com.fuckmail.desktop"
    MACOSX_BUNDLE_BUNDLE_VERSION "0.1.0"
    MACOSX_BUNDLE_SHORT_VERSION_STRING "0.1.0"
)
# Qt's Windows runtime layout differs from the Unix subdirectory layout. Let
# developer launchers consume the generator's actual path rather than guess it.
file(GENERATE OUTPUT "${CMAKE_BINARY_DIR}/gui-executable-path.txt"
    CONTENT "$<TARGET_FILE:fuckmail-gui>\n")
if(BUILD_TESTING)
    if(WIN32)
        add_executable(gui-ipc-trust-test IpcTrustWindowsTest.cpp IpcTrust.cpp)
        target_link_libraries(gui-ipc-trust-test PRIVATE advapi32)
    else()
        add_executable(gui-ipc-trust-test IpcTrustTest.cpp IpcTrust.cpp)
    endif()
    target_link_libraries(gui-ipc-trust-test PRIVATE Qt6::Core Qt6::Network)
    add_test(NAME gui-ipc-trust COMMAND gui-ipc-trust-test)
    set_tests_properties(gui-ipc-trust PROPERTIES TIMEOUT 10)
endif()
if(BUILD_TESTING)
    add_executable(gui-composer-probe ComposerProbe.cpp)
    target_link_libraries(gui-composer-probe PRIVATE Qt6::Gui)
    add_test(NAME gui-composer-probe COMMAND gui-composer-probe)
    set_tests_properties(gui-composer-probe PROPERTIES TIMEOUT 10 ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
endif()

if(BUILD_TESTING)
    add_executable(gui-reply-document-test ReplyDocumentTest.cpp ReplyDocument.cpp ReplyDocument.h)
    target_link_libraries(gui-reply-document-test PRIVATE Qt6::Quick)
    add_test(NAME gui-reply-document COMMAND gui-reply-document-test)
    set_tests_properties(gui-reply-document PROPERTIES TIMEOUT 10 ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
endif()
if(BUILD_TESTING)
    add_executable(gui-calendar-layout-test CalendarLayoutTest.cpp CalendarLayout.cpp)
    target_link_libraries(gui-calendar-layout-test PRIVATE Qt6::Core)
    add_test(NAME gui-calendar-layout COMMAND gui-calendar-layout-test)
    set_tests_properties(gui-calendar-layout PROPERTIES TIMEOUT 10)
endif()
