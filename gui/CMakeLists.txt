qt_add_executable(fuckmail-gui WIN32 MACOSX_BUNDLE
    main.cpp
    AccountLifecycleBackend.cpp
    MailBackend.cpp
    GoogleBackend.cpp
    GoogleLabels.cpp
    GoogleOperation.cpp
    GoogleOperation.h
    MailBackend.h
    Phase1Backend.cpp
    CompositionBackend.cpp
    ConversationListModel.cpp
    ConversationListModel.h
    DraftAttachmentBackend.cpp
    GoogleDraftBackend.cpp
    GoogleDraftSnapshotBackend.cpp GoogleDraftAuthoringBackend.cpp
    GoogleSendBackend.cpp
    CalendarOperations.cpp
    ReplyDocument.cpp
    ReplyDocument.h
    CalendarLayout.cpp
    CalendarLayout.h
    HatchItem.cpp
    HatchItem.h
    GuiSelfTest.cpp
    GuiSelfTest.h
    GuiCalendarSelfTest.cpp
    GuiCalendarSelfTest.h
    GuiCompositionSelfTest.cpp
    GuiCompositionSelfTest.h
    GuiGoogleDraftActionsSelfTest.cpp
    GuiGoogleDraftActionsSelfTest.h
    GuiGoogleCalendarSelfTest.cpp
    GuiGoogleCalendarSelfTest.h
    GuiAccountLifecycleSelfTest.cpp
    GuiAccountLifecycleSelfTest.h
    GuiGoogleDraftSnapshotSelfTest.cpp
    GuiGoogleDraftSnapshotSelfTest.h
    GuiSettingsSelfTest.cpp
    GuiSettingsSelfTest.h
    GuiMailNavigationSelfTest.cpp
    GuiMailNavigationSelfTest.h
    GuiSearchBenchmark.cpp
    GuiSearchBenchmark.h
    GuiPaintObserver.h
    GuiInteractionBenchmark.cpp
    GuiInteractionBenchmark.h
    GuiResizeBenchmark.cpp
    GuiResizeBenchmark.h
    IpcTrust.cpp
    IpcTrust.h
)
set_source_files_properties(Theme.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)
qt_add_qml_module(fuckmail-gui
    URI FuckMail
    VERSION 1.0
    QML_FILES Main.qml AccountLifecycleDialogs.qml Theme.qml FmButton.qml FmToolTip.qml FmIcon.qml FmTextField.qml FmComboBox.qml MailView.qml MailFilters.qml SearchPopup.qml FmSegmented.qml RecipientField.qml ReplyComposer.qml SenderCard.qml CalendarView.qml EventDetails.qml EventEditor.qml CalendarOperations.qml CalendarEventList.qml SettingsView.qml GoogleLabelSetup.qml TodayStrip.qml
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
if(BUILD_TESTING)
    add_executable(gui-conversation-list-model-test ConversationListModelTest.cpp ConversationListModel.cpp ConversationListModel.h)
    target_link_libraries(gui-conversation-list-model-test PRIVATE Qt6::Core Qt6::Test)
    add_test(NAME gui-conversation-list-model COMMAND gui-conversation-list-model-test)
    set_tests_properties(gui-conversation-list-model PROPERTIES TIMEOUT 15)
endif()
if(BUILD_TESTING)
    add_executable(gui-calendar-controls-test CalendarControlsTest.cpp)
    set_target_properties(gui-calendar-controls-test PROPERTIES AUTOMOC ON)
    target_link_libraries(gui-calendar-controls-test PRIVATE Qt6::Quick Qt6::QuickControls2)
    target_compile_definitions(gui-calendar-controls-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    add_test(NAME gui-calendar-controls COMMAND gui-calendar-controls-test)
    set(FM_CONTROLS_TEST_ENV "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software;QT_FORCE_STDERR_LOGGING=1")
    if(WIN32)
        list(APPEND FM_CONTROLS_TEST_ENV "QT_QPA_FONTDIR=$ENV{SystemRoot}/Fonts")
    endif()
    set_tests_properties(gui-calendar-controls PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    add_executable(gui-google-operation-test GoogleOperationTest.cpp GoogleOperation.cpp IpcTrust.cpp)
    target_link_libraries(gui-google-operation-test PRIVATE Qt6::Core Qt6::Network)
    if(WIN32)
        target_link_libraries(gui-google-operation-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-google-operation COMMAND gui-google-operation-test)
    set_tests_properties(gui-google-operation PROPERTIES TIMEOUT 30 ENVIRONMENT "QT_FORCE_STDERR_LOGGING=1")
    add_executable(gui-google-draft-controls-test GoogleDraftControlsTest.cpp GoogleOperationTestFixture.h
        AccountLifecycleBackend.cpp
    MailBackend.cpp MailBackend.h GoogleBackend.cpp GoogleDraftBackend.cpp GoogleDraftSnapshotBackend.cpp GoogleDraftAuthoringBackend.cpp GoogleSendBackend.cpp GoogleLabels.cpp GoogleOperation.cpp GoogleOperation.h
        Phase1Backend.cpp CompositionBackend.cpp DraftAttachmentBackend.cpp CalendarOperations.cpp
        ConversationListModel.cpp ConversationListModel.h CalendarLayout.cpp CalendarLayout.h
        ReplyDocument.cpp ReplyDocument.h IpcTrust.cpp IpcTrust.h)
    target_link_libraries(gui-google-draft-controls-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-google-draft-controls-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-google-draft-controls-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-google-draft-controls-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-google-draft-controls COMMAND gui-google-draft-controls-test)
    set_tests_properties(gui-google-draft-controls PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    add_executable(gui-google-send-controls-test GoogleSendControlsTest.cpp GoogleOperationTestFixture.h
        AccountLifecycleBackend.cpp
    MailBackend.cpp MailBackend.h GoogleBackend.cpp GoogleDraftBackend.cpp GoogleDraftSnapshotBackend.cpp GoogleDraftAuthoringBackend.cpp GoogleSendBackend.cpp GoogleLabels.cpp GoogleOperation.cpp GoogleOperation.h
        Phase1Backend.cpp CompositionBackend.cpp DraftAttachmentBackend.cpp CalendarOperations.cpp
        ConversationListModel.cpp ConversationListModel.h CalendarLayout.cpp CalendarLayout.h
        ReplyDocument.cpp ReplyDocument.h IpcTrust.cpp IpcTrust.h)
    target_link_libraries(gui-google-send-controls-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-google-send-controls-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-google-send-controls-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-google-send-controls-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-google-send-controls COMMAND gui-google-send-controls-test)
    set_tests_properties(gui-google-send-controls PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
endif()

if(BUILD_TESTING)
    add_executable(gui-google-draft-snapshot-controls-test GoogleDraftSnapshotControlsTest.cpp GoogleOperationTestFixture.h
        GuiGoogleDraftSnapshotSelfTest.cpp GuiGoogleDraftSnapshotSelfTest.h
        AccountLifecycleBackend.cpp
    MailBackend.cpp MailBackend.h GoogleBackend.cpp GoogleDraftBackend.cpp GoogleDraftSnapshotBackend.cpp GoogleDraftAuthoringBackend.cpp GoogleSendBackend.cpp GoogleLabels.cpp GoogleOperation.cpp GoogleOperation.h
        Phase1Backend.cpp CompositionBackend.cpp DraftAttachmentBackend.cpp CalendarOperations.cpp
        ConversationListModel.cpp ConversationListModel.h CalendarLayout.cpp CalendarLayout.h
        ReplyDocument.cpp ReplyDocument.h IpcTrust.cpp IpcTrust.h)
    target_link_libraries(gui-google-draft-snapshot-controls-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-google-draft-snapshot-controls-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-google-draft-snapshot-controls-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-google-draft-snapshot-controls-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-google-draft-snapshot-controls COMMAND gui-google-draft-snapshot-controls-test)
    set_tests_properties(gui-google-draft-snapshot-controls PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    get_target_property(FM_AUTHORING_TEST_SOURCES gui-google-draft-snapshot-controls-test SOURCES)
    list(REMOVE_ITEM FM_AUTHORING_TEST_SOURCES GoogleDraftSnapshotControlsTest.cpp)
    add_executable(gui-google-draft-authoring-controls-test GoogleDraftAuthoringControlsTest.cpp ${FM_AUTHORING_TEST_SOURCES})
    target_link_libraries(gui-google-draft-authoring-controls-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-google-draft-authoring-controls-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-google-draft-authoring-controls-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-google-draft-authoring-controls-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-google-draft-authoring-controls COMMAND gui-google-draft-authoring-controls-test)
    set_tests_properties(gui-google-draft-authoring-controls PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    add_executable(gui-account-lifecycle-controls-test AccountLifecycleControlsTest.cpp ${FM_AUTHORING_TEST_SOURCES})
    target_link_libraries(gui-account-lifecycle-controls-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-account-lifecycle-controls-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-account-lifecycle-controls-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-account-lifecycle-controls-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-account-lifecycle-controls COMMAND gui-account-lifecycle-controls-test)
    set_tests_properties(gui-account-lifecycle-controls PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
endif()
