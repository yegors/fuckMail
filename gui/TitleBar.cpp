// matchTitleBar for Windows and Linux: sets Qt's dark or light colour scheme and,
// on Windows 11, the DWM caption colour to the window's background.
#include "TitleBar.h"

#include <QGuiApplication>
#include <QQuickWindow>
#include <QStyleHints>

#ifdef Q_OS_WIN
#include <windows.h>
#include <dwmapi.h>
#endif

void matchTitleBar(QQuickWindow *window) {
    const QColor color = window->color();
    // Qt switches the frame and caption buttons between dark and light.
    QGuiApplication::styleHints()->setColorScheme(color.lightnessF() < 0.5 ? Qt::ColorScheme::Dark : Qt::ColorScheme::Light);
#ifdef Q_OS_WIN
    if (QGuiApplication::platformName() != QLatin1String("windows"))
        return;
    // Windows 11 only; Windows 10 keeps its default dark or light caption.
    const COLORREF caption = RGB(color.red(), color.green(), color.blue());
    DwmSetWindowAttribute(reinterpret_cast<HWND>(window->winId()), DWMWA_CAPTION_COLOR, &caption, sizeof caption);
#endif
}
