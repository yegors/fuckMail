// matchTitleBar for macOS: sets Qt's dark or light colour scheme and paints the
// transparent title bar by setting the NSWindow background to the window colour.
#include "TitleBar.h"

#include <QGuiApplication>
#include <QQuickWindow>
#include <QStyleHints>

#import <AppKit/AppKit.h>

void matchTitleBar(QQuickWindow *window) {
    const QColor color = window->color();
    // Qt sets the app appearance, so the title text reads on the colour.
    QGuiApplication::styleHints()->setColorScheme(color.lightnessF() < 0.5 ? Qt::ColorScheme::Dark : Qt::ColorScheme::Light);
    if (QGuiApplication::platformName() != QLatin1String("cocoa"))
        return;
    // A transparent title bar shows the NSWindow background. Qt owns the
    // transparency and resets it from this flag, so set the flag, not AppKit.
    if (!window->flags().testFlag(Qt::NoTitleBarBackgroundHint))
        window->setFlag(Qt::NoTitleBarBackgroundHint);
    NSWindow *native = [reinterpret_cast<NSView *>(window->winId()) window];
    native.backgroundColor = [NSColor colorWithSRGBRed:color.redF() green:color.greenF() blue:color.blueF() alpha:1];
}
