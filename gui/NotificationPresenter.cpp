#include "NotificationPresenter.h"
#include "MailBackend.h"
#include <QApplication>
#include <QPainter>
#include <QPixmap>
#include <QWindow>

NotificationPresenter::NotificationPresenter(MailBackend *backend, QObject *parent) : QObject(parent), m_tray(this) {
    QPixmap icon(32, 32); icon.fill(Qt::transparent);
    QPainter painter(&icon); painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(QPen(QColor("#e7e5e0"), 2)); painter.setBrush(QColor("#242522"));
    painter.drawRoundedRect(QRectF(2, 6, 28, 20), 4, 4);
    painter.drawLine(QPointF(3, 8), QPointF(16, 18)); painter.drawLine(QPointF(16, 18), QPointF(29, 8)); painter.end();
    m_tray.setIcon(QIcon(icon)); m_tray.setToolTip("fuckMail");
    const auto showWindow = [] {
        for (auto window : QGuiApplication::topLevelWindows()) if (window->objectName() == "mainWindow") {
            window->show(); window->raise(); window->requestActivate(); break;
        }
    };
    connect(&m_tray, &QSystemTrayIcon::messageClicked, this, showWindow);
    connect(&m_tray, &QSystemTrayIcon::activated, this, [showWindow](auto reason) {
        if (reason == QSystemTrayIcon::Trigger || reason == QSystemTrayIcon::DoubleClick) showWindow();
    });
    connect(backend, &MailBackend::desktopNotification, this, [this](const QString &title, const QString &body) {
        if (!QSystemTrayIcon::isSystemTrayAvailable() || !QSystemTrayIcon::supportsMessages()) return;
        m_tray.show(); m_tray.showMessage(title, body, QSystemTrayIcon::Information, 10000);
    });
    connect(backend, &MailBackend::shutdownReady, &m_tray, &QSystemTrayIcon::hide);
}
