#pragma once

#include <QObject>

// Whether the operating system asks apps to reduce motion. QML reads it as
// `systemMotion.reduced` when the motion preference is "system". It is read
// at startup and again whenever the app comes back to the front, so a change
// in system settings applies without a restart.
// SystemMotion.mm on macOS, SystemMotion.cpp elsewhere.
class SystemMotion : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool reduced READ reduced NOTIFY reducedChanged)
public:
    explicit SystemMotion(QObject *parent = nullptr);
    bool reduced() const { return m_reduced; }
signals:
    void reducedChanged();
private:
    void refresh();
    void setReduced(bool reduced);
    bool m_reduced = false;
    bool m_reading = false;
};
