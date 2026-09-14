#pragma once
#include <QObject>
#include <QSystemTrayIcon>
class MailBackend;
class NotificationPresenter final : public QObject {
public:
    explicit NotificationPresenter(MailBackend *backend, QObject *parent = nullptr);
private:
    QSystemTrayIcon m_tray;
};
