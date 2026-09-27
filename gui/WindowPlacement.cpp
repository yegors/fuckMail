#include "WindowPlacement.h"
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSaveFile>
#include <QScreen>
#include <QWindow>

WindowPlacement::WindowPlacement(const QString &profile, const QSize &minimum, QObject *parent)
    : QObject(parent), m_path(profile.isEmpty() ? QString() : QDir(profile).filePath("window.json")), m_minimum(minimum) {
    // Maximizing can report the new size before the new state, so a normal
    // geometry only counts once the window has stayed unmaximized briefly.
    m_settle.setSingleShot(true);
    m_settle.setInterval(250);
    connect(&m_settle, &QTimer::timeout, this, &WindowPlacement::settle);
    if (m_path.isEmpty()) return;
    QFile file(m_path);
    if (!file.open(QIODevice::ReadOnly)) return;
    const auto object = QJsonDocument::fromJson(file.read(4096)).object();
    const QRect normal(object.value("x").toInt(), object.value("y").toInt(), object.value("width").toInt(), object.value("height").toInt());
    if (normal.width() > 0 && normal.height() > 0) m_normal = normal;
    m_maximized = object.value("maximized").toBool();
    m_mailSplitWidth = qBound(0.0, object.value("mail_split_width").toDouble(), 10000.0);
}

void WindowPlacement::setMailSplitWidth(qreal width) {
    width = qBound(0.0, qreal(qRound(width)), 10000.0);
    if (width == m_mailSplitWidth) return;
    m_mailSplitWidth = width;
    emit mailSplitWidthChanged();
    save();
}

QRect WindowPlacement::fit(const QRect &saved, const QList<QRect> &screens, const QSize &minimum) {
    if (saved.isEmpty()) return {};
    // The strip where the title bar sits must be reachable to move the window.
    const QRect title(saved.x(), saved.y(), saved.width(), qMin(saved.height(), 40));
    for (const auto &screen : screens) {
        const auto overlap = screen.intersected(title);
        if (overlap.width() < qMin(120, saved.width()) || overlap.height() < qMin(20, title.height())) continue;
        const QSize size(qBound(qMin(minimum.width(), screen.width()), saved.width(), screen.width()),
            qBound(qMin(minimum.height(), screen.height()), saved.height(), screen.height()));
        // Keep a capped window inside the screen it was on.
        const int x = qBound(screen.left(), saved.x(), screen.right() + 1 - size.width());
        const int y = qBound(screen.top(), saved.y(), screen.bottom() + 1 - size.height());
        return QRect(QPoint(x, y), size);
    }
    return {};
}

QVariantMap WindowPlacement::initialProperties() const {
    QList<QRect> screens;
    for (auto screen : QGuiApplication::screens()) screens.append(screen->availableGeometry());
    const auto rect = fit(m_normal, screens, m_minimum);
    if (rect.isEmpty()) return {};
    QVariantMap properties{{"x", rect.x()}, {"y", rect.y()}, {"width", rect.width()}, {"height", rect.height()}};
    if (m_maximized) properties.insert("visibility", QWindow::Maximized);
    return properties;
}

void WindowPlacement::track(QWindow *window) {
    m_window = window;
    if (!window) return;
    const auto moved = [this] { m_settle.start(); };
    connect(window, &QWindow::xChanged, this, moved);
    connect(window, &QWindow::yChanged, this, moved);
    connect(window, &QWindow::widthChanged, this, moved);
    connect(window, &QWindow::heightChanged, this, moved);
    connect(window, &QWindow::windowStateChanged, this, moved);
    // A window that opens maximized has not shown its normal geometry yet;
    // keep the saved one until it is unmaximized.
    if (window->windowStates() == Qt::WindowNoState) m_normal = window->geometry();
    connect(qApp, &QGuiApplication::aboutToQuit, this, [this] { settle(); });
}

void WindowPlacement::settle() {
    if (!m_window) return;
    const auto states = m_window->windowStates();
    // Minimized and full-screen windows reopen as they were before.
    if (states & (Qt::WindowMinimized | Qt::WindowFullScreen)) return;
    m_maximized = states & Qt::WindowMaximized;
    if (!m_maximized) m_normal = m_window->geometry();
    save();
}

void WindowPlacement::save() {
    // A new profile belongs to the engine until it has created the directory.
    if (m_path.isEmpty() || m_normal.isEmpty() || !QFileInfo(m_path).dir().exists()) return;
    QJsonObject object{{"x", m_normal.x()}, {"y", m_normal.y()}, {"width", m_normal.width()}, {"height", m_normal.height()}, {"maximized", m_maximized}};
    if (m_mailSplitWidth > 0) object.insert("mail_split_width", m_mailSplitWidth);
    QSaveFile file(m_path);
    if (!file.open(QIODevice::WriteOnly)) return;
    file.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
    file.commit();
}
