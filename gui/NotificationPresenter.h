// Shows MailBackend's desktopNotification signals as system tray messages;
// clicking the message or the tray icon raises the main window.
#pragma once
#include <QObject>
#include <QSystemTrayIcon>
class MailBackend;
/// main.cpp creates one on the stack next to MailBackend for the GUI's
/// lifetime. GUI thread only. It has no API: it only listens to `backend`'s
/// signals. `backend` must outlive it.
///
/// Delivery goes through QSystemTrayIcon, which Qt maps to each platform's
/// own mechanism (a notification-area toast on Windows, Notification Center
/// on macOS, the tray or notification service of the Linux desktop). Where
/// Qt reports no tray or no message support, as on some Linux desktops,
/// notifications are dropped silently.
class NotificationPresenter final : public QObject {
public:
    /// Connects to `backend`'s desktopNotification and shutdownReady signals.
    /// The tray icon stays hidden until the first notification is shown.
    explicit NotificationPresenter(MailBackend *backend, QObject *parent = nullptr);
private:
    QSystemTrayIcon m_tray;
};
