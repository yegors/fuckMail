// SystemMotion for macOS: System Settings → Accessibility → Display →
// Reduce motion.
#include "SystemMotion.h"

#include <QGuiApplication>

#import <AppKit/AppKit.h>

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
    setReduced([[NSWorkspace sharedWorkspace] accessibilityDisplayShouldReduceMotion]);
}
