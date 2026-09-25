include(cmake/litehtml.cmake)
# The HTML reader's litehtml item.
add_library(fm-gui-html STATIC FmHtmlView.cpp FmHtmlView.h HtmlDocument.cpp HtmlDocument.h)
target_link_libraries(fm-gui-html PUBLIC Qt6::Quick PRIVATE fm-litehtml)
target_compile_features(fm-gui-html PUBLIC cxx_std_20)
# Its engine calls. HtmlContent.cpp defines MailBackend::htmlContent, so only
# programs that compile MailBackend use this library.
add_library(fm-gui-html-content STATIC HtmlContent.cpp HtmlContent.h)
target_link_libraries(fm-gui-html-content PUBLIC Qt6::Quick Qt6::Network)
target_compile_features(fm-gui-html-content PUBLIC cxx_std_20)
# FmIcon.qml wraps this item; every process that loads it registers the type first.
add_library(fm-gui-icons STATIC FmIconItem.cpp FmIconItem.h)
target_link_libraries(fm-gui-icons PUBLIC Qt6::Quick fm-gui-html)
target_compile_features(fm-gui-icons PUBLIC cxx_std_20)
qt_add_executable(fuckmail-gui WIN32 MACOSX_BUNDLE
    main.cpp
    DebugConsole.cpp DebugConsole.h
    NotificationPresenter.cpp
    NotificationPresenter.h
    AccountLifecycleBackend.cpp
    MailBackend.cpp
    GoogleBackend.cpp
    GoogleLabels.cpp
    GoogleOperation.cpp
    GoogleOperation.h
    MailBackend.h
    Phase1Backend.cpp
    CompositionBackend.cpp DraftTransferBackend.cpp
    ConversationListModel.cpp
    ConversationListModel.h
    DraftAttachmentBackend.cpp
    GoogleDraftBackend.cpp
    GoogleDraftSnapshotBackend.cpp GoogleDraftAuthoringBackend.cpp
    GoogleSendBackend.cpp SendUndoBackend.cpp OutgoingBackend.cpp DiscardUndoBackend.cpp CalendarUndoBackend.cpp
    CalendarOperations.cpp CalendarSearchBackend.cpp
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
    GuiBenchmarkSupport.h
    GuiInteractionBenchmark.cpp
    GuiInteractionBenchmark.h
    GuiResizeBenchmark.cpp
    GuiResizeBenchmark.h
    GuiCalendarBenchmark.cpp
    GuiCalendarBenchmark.h
    IpcTrust.cpp
    IpcTrust.h
)
set_source_files_properties(Theme.qml ShortcutRegistry.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)
qt_add_qml_module(fuckmail-gui
    URI FuckMail
    VERSION 1.0
    QML_FILES DebugConsole.qml AccountAnalysis.qml
    QML_FILES AccountProgress.qml AccountPolicy.qml AccountCard.qml AccountHoverCard.qml AccountColorPicker.qml LabelsDialog.qml SignatureDialog.qml SpamState.qml SpamList.qml SpamReader.qml
    QML_FILES HtmlMessageBody.qml RemoteContentSettings.qml
    QML_FILES CalendarMailCount.qml
    QML_FILES FmScrollBar.qml Main.qml StartupOverlay.qml ReminderPopup.qml AccountLifecycleDialogs.qml Theme.qml ShortcutRegistry.qml ShortcutSettings.qml ActionToast.qml FmButton.qml FmMenu.qml FmMenuItem.qml FmDialog.qml FmConfirmDialog.qml FmToolTip.qml FmIcon.qml FmTextField.qml FmDateField.qml FmSwitch.qml FmCheckBox.qml FmComboBox.qml MailView.qml SearchPopup.qml CalendarSearchPopup.qml FmSegmented.qml RecipientField.qml ReplyComposer.qml SenderCard.qml CalendarView.qml CalendarOccurrenceRead.qml EventDetails.qml EventEditor.qml CalendarOperations.qml CalendarEventList.qml SettingsView.qml GoogleConnectionDialog.qml LocalModelSettings.qml MailAssistance.qml MessageSource.qml AccountBadge.qml MessageHeaderButton.qml AccountEditor.qml AttachmentTile.qml AttachmentViewer.qml SummaryCard.qml SummarizeAction.qml MessageAssistance.qml BlockedDomainsSettings.qml LinkCard.qml GoogleLabelSetup.qml LabelSettings.qml TodayStrip.qml
    RESOURCES fonts/Geist-Regular.ttf fonts/Geist-Medium.ttf fonts/Geist-SemiBold.ttf fonts/LICENSE-Geist.txt
)
target_link_libraries(fuckmail-gui PRIVATE fm-gui-icons fm-gui-html-content Qt6::Quick Qt6::QuickControls2 Qt6::Network Qt6::Widgets Qt6::Multimedia)
target_compile_features(fuckmail-gui PRIVATE cxx_std_20)
if(WIN32)
    target_link_libraries(fuckmail-gui PRIVATE advapi32 winmm)
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
    link_libraries(Qt6::Widgets fm-gui-icons fm-gui-html-content)
    if(WIN32)
        link_libraries(winmm)
    endif()
    add_executable(gui-debug-console-test DebugConsoleTest.cpp DebugConsole.cpp DebugConsole.h)
    target_link_libraries(gui-debug-console-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Test)
    target_compile_definitions(gui-debug-console-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    add_test(NAME gui-debug-console COMMAND gui-debug-console-test)
    set_tests_properties(gui-debug-console PROPERTIES TIMEOUT 30 ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software;QT_FORCE_STDERR_LOGGING=1")
    if(WIN32)
        set_property(TEST gui-debug-console APPEND PROPERTY ENVIRONMENT "QT_QPA_FONTDIR=$ENV{SystemRoot}/Fonts")
    endif()
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
    add_executable(gui-calendar-controls-test CalendarControlsTest.cpp CalendarLayout.cpp CalendarLayout.h)
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
    MailBackend.cpp MailBackend.h GoogleBackend.cpp GoogleDraftBackend.cpp GoogleDraftSnapshotBackend.cpp GoogleDraftAuthoringBackend.cpp GoogleSendBackend.cpp SendUndoBackend.cpp OutgoingBackend.cpp DiscardUndoBackend.cpp CalendarUndoBackend.cpp GoogleLabels.cpp GoogleOperation.cpp GoogleOperation.h
        Phase1Backend.cpp CompositionBackend.cpp DraftTransferBackend.cpp DraftAttachmentBackend.cpp CalendarOperations.cpp CalendarSearchBackend.cpp
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
    MailBackend.cpp MailBackend.h GoogleBackend.cpp GoogleDraftBackend.cpp GoogleDraftSnapshotBackend.cpp GoogleDraftAuthoringBackend.cpp GoogleSendBackend.cpp SendUndoBackend.cpp OutgoingBackend.cpp DiscardUndoBackend.cpp CalendarUndoBackend.cpp GoogleLabels.cpp GoogleOperation.cpp GoogleOperation.h
        Phase1Backend.cpp CompositionBackend.cpp DraftTransferBackend.cpp DraftAttachmentBackend.cpp CalendarOperations.cpp CalendarSearchBackend.cpp
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
    add_executable(gui-calendar-occurrence-test CalendarOccurrenceBackendTest.cpp GoogleOperationTestFixture.h
        AccountLifecycleBackend.cpp MailBackend.cpp MailBackend.h GoogleBackend.cpp GoogleDraftBackend.cpp
        GoogleDraftSnapshotBackend.cpp GoogleDraftAuthoringBackend.cpp GoogleSendBackend.cpp SendUndoBackend.cpp OutgoingBackend.cpp DiscardUndoBackend.cpp CalendarUndoBackend.cpp GoogleLabels.cpp GoogleOperation.cpp GoogleOperation.h
        Phase1Backend.cpp CompositionBackend.cpp DraftTransferBackend.cpp DraftAttachmentBackend.cpp CalendarOperations.cpp CalendarSearchBackend.cpp
        ConversationListModel.cpp ConversationListModel.h CalendarLayout.cpp CalendarLayout.h ReplyDocument.cpp ReplyDocument.h IpcTrust.cpp IpcTrust.h)
    target_link_libraries(gui-calendar-occurrence-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-calendar-occurrence-test PRIVATE cxx_std_20)
    if(WIN32)
        target_link_libraries(gui-calendar-occurrence-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-calendar-occurrence COMMAND gui-calendar-occurrence-test)
    set_tests_properties(gui-calendar-occurrence PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    get_target_property(FM_NAVIGATION_TEST_SOURCES gui-calendar-occurrence-test SOURCES)
    list(REMOVE_ITEM FM_NAVIGATION_TEST_SOURCES CalendarOccurrenceBackendTest.cpp)
    add_executable(gui-calendar-mail-activity-test CalendarMailActivityTest.cpp ${FM_NAVIGATION_TEST_SOURCES})
    target_link_libraries(gui-calendar-mail-activity-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-calendar-mail-activity-test PRIVATE cxx_std_20)
    if(WIN32)
        target_link_libraries(gui-calendar-mail-activity-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-calendar-mail-activity COMMAND gui-calendar-mail-activity-test)
    set_tests_properties(gui-calendar-mail-activity PROPERTIES TIMEOUT 20 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    add_executable(gui-mail-selection-test MailSelectionTest.cpp ${FM_NAVIGATION_TEST_SOURCES})
    add_executable(gui-composer-input-test ComposerInputTest.cpp ${FM_NAVIGATION_TEST_SOURCES})
    target_link_libraries(gui-composer-input-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-composer-input-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-composer-input-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-composer-input-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-composer-input COMMAND gui-composer-input-test)
    set_tests_properties(gui-composer-input PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    add_executable(gui-send-undo-test SendUndoTest.cpp ${FM_NAVIGATION_TEST_SOURCES})
    target_link_libraries(gui-send-undo-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-send-undo-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-send-undo-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-send-undo-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-send-undo COMMAND gui-send-undo-test)
    set_tests_properties(gui-send-undo PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    add_executable(gui-draft-discard-test DraftDiscardTest.cpp ${FM_NAVIGATION_TEST_SOURCES})
    target_link_libraries(gui-draft-discard-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-draft-discard-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-draft-discard-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-draft-discard-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-draft-discard COMMAND gui-draft-discard-test)
    set_tests_properties(gui-draft-discard PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    add_executable(gui-calendar-undo-test CalendarUndoTest.cpp ${FM_NAVIGATION_TEST_SOURCES})
    target_link_libraries(gui-calendar-undo-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-calendar-undo-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-calendar-undo-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-calendar-undo-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-calendar-undo COMMAND gui-calendar-undo-test)
    set_tests_properties(gui-calendar-undo PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    add_executable(gui-draft-transfer-test DraftTransferControlsTest.cpp ${FM_NAVIGATION_TEST_SOURCES})
    target_link_libraries(gui-draft-transfer-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-draft-transfer-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-draft-transfer-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-draft-transfer-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-draft-transfer COMMAND gui-draft-transfer-test)
    set_tests_properties(gui-draft-transfer PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    target_link_libraries(gui-mail-selection-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-mail-selection-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-mail-selection-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-mail-selection-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-mail-selection COMMAND gui-mail-selection-test)
    set_tests_properties(gui-mail-selection PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    add_executable(gui-mail-navigation-cache-test MailNavigationCacheTest.cpp ${FM_NAVIGATION_TEST_SOURCES})
    target_link_libraries(gui-mail-navigation-cache-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-mail-navigation-cache-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-mail-navigation-cache-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-mail-navigation-cache-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-mail-navigation-cache COMMAND gui-mail-navigation-cache-test)
    set_tests_properties(gui-mail-navigation-cache PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    add_executable(gui-calendar-search-test CalendarSearchTest.cpp HatchItem.cpp HatchItem.h ${FM_NAVIGATION_TEST_SOURCES})
    target_link_libraries(gui-calendar-search-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-calendar-search-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-calendar-search-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-calendar-search-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-calendar-search COMMAND gui-calendar-search-test)
    set_tests_properties(gui-calendar-search PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    add_executable(gui-notification-backend-test NotificationBackendTest.cpp ${FM_NAVIGATION_TEST_SOURCES})
    target_link_libraries(gui-notification-backend-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Network)
    target_compile_features(gui-notification-backend-test PRIVATE cxx_std_20)
    target_compile_definitions(gui-notification-backend-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    if(WIN32)
        target_link_libraries(gui-notification-backend-test PRIVATE advapi32)
    endif()
    add_test(NAME gui-notification-backend COMMAND gui-notification-backend-test)
    set_tests_properties(gui-notification-backend PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
endif()


if(BUILD_TESTING)
    add_executable(gui-google-draft-snapshot-controls-test GoogleDraftSnapshotControlsTest.cpp GoogleOperationTestFixture.h
        GuiGoogleDraftSnapshotSelfTest.cpp GuiGoogleDraftSnapshotSelfTest.h
        AccountLifecycleBackend.cpp
    MailBackend.cpp MailBackend.h GoogleBackend.cpp GoogleDraftBackend.cpp GoogleDraftSnapshotBackend.cpp GoogleDraftAuthoringBackend.cpp GoogleSendBackend.cpp SendUndoBackend.cpp OutgoingBackend.cpp DiscardUndoBackend.cpp CalendarUndoBackend.cpp GoogleLabels.cpp GoogleOperation.cpp GoogleOperation.h
        Phase1Backend.cpp CompositionBackend.cpp DraftTransferBackend.cpp DraftAttachmentBackend.cpp CalendarOperations.cpp CalendarSearchBackend.cpp
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

if(BUILD_TESTING)
    add_executable(gui-local-model-test LocalModelTest.cpp)
    target_link_libraries(gui-local-model-test PRIVATE Qt6::Quick Qt6::QuickControls2)
    target_compile_definitions(gui-local-model-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    add_test(NAME gui-local-model COMMAND gui-local-model-test)
    set_tests_properties(gui-local-model PROPERTIES TIMEOUT 30 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
endif()

if(BUILD_TESTING)
    add_executable(gui-google-connection-controls-test GoogleConnectionControlsTest.cpp)
    target_link_libraries(gui-google-connection-controls-test PRIVATE Qt6::Quick Qt6::QuickControls2)
    target_compile_definitions(gui-google-connection-controls-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    add_test(NAME gui-google-connection-controls COMMAND gui-google-connection-controls-test)
    set_tests_properties(gui-google-connection-controls PROPERTIES TIMEOUT 15 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
endif()

if(BUILD_TESTING)
    add_executable(gui-label-controls-test LabelControlsTest.cpp)
    target_link_libraries(gui-label-controls-test PRIVATE Qt6::Quick Qt6::QuickControls2)
    target_compile_definitions(gui-label-controls-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    add_test(NAME gui-label-controls COMMAND gui-label-controls-test)
    set_tests_properties(gui-label-controls PROPERTIES TIMEOUT 20 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
endif()

if(BUILD_TESTING)
    add_executable(gui-html-view-test HtmlViewTest.cpp HtmlLinks.h)
    target_link_libraries(gui-html-view-test PRIVATE Qt6::Quick Qt6::Test)
    add_test(NAME gui-html-view COMMAND gui-html-view-test)
    set_tests_properties(gui-html-view PROPERTIES TIMEOUT 60 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
    # Developer tool for rendering a local corpus of sanitised messages; not a test.
    add_executable(fm-html-probe HtmlProbe.cpp)
    target_link_libraries(fm-html-probe PRIVATE fm-gui-html Qt6::Gui)
endif()

if(BUILD_TESTING)
    add_executable(gui-html-message-body-test HtmlMessageBodyTest.cpp)
    target_link_libraries(gui-html-message-body-test PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Test)
    target_compile_definitions(gui-html-message-body-test PRIVATE FM_GUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}")
    add_test(NAME gui-html-message-body COMMAND gui-html-message-body-test)
    set_tests_properties(gui-html-message-body PROPERTIES TIMEOUT 60 ENVIRONMENT "${FM_CONTROLS_TEST_ENV}")
endif()
