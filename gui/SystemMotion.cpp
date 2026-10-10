// SystemMotion for Windows and Linux. Windows reads Settings → Accessibility →
// Visual effects → Animation effects; Linux reads GNOME's enable-animations
// key through gsettings, off the main thread, and keeps full motion when the
// key or the tool is missing.
#include "SystemMotion.h"

#include <QGuiApplication>
#include <QProcess>

#ifdef Q_OS_WIN
#include <windows.h>
#endif

SystemMotion::SystemMotion(QObject *parent) : QObject(parent) {
    refresh();
    connect(qGuiApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState state) {
        if (state == Qt::ApplicationActive) refresh();
    });
}

void SystemMotion::setReduced(bool reduced) {
    if (m_reduced == reduced) return;
    m_reduced = reduced;
    emit reducedChanged();
}

void SystemMotion::refresh() {
#ifdef Q_OS_WIN
    BOOL animate = TRUE;
    if (SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &animate, 0)) setReduced(!animate);
#else
    if (m_reading) return;
    m_reading = true;
    auto *process = new QProcess(this);
    connect(process, &QProcess::finished, this, [this, process](int code, QProcess::ExitStatus status) {
        m_reading = false;
        if (status == QProcess::NormalExit && code == 0) setReduced(process->readAllStandardOutput().trimmed() == "false");
        process->deleteLater();
    });
    connect(process, &QProcess::errorOccurred, this, [this, process](QProcess::ProcessError error) {
        if (error != QProcess::FailedToStart) return;
        m_reading = false;
        process->deleteLater();
    });
    process->start(QStringLiteral("gsettings"), {QStringLiteral("get"), QStringLiteral("org.gnome.desktop.interface"), QStringLiteral("enable-animations")});
#endif
}
