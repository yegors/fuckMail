// bringToFront and holdReminderTiming for Windows and Linux. Windows gets past
// its foreground lock by sharing the foreground thread's input and briefly
// making the window topmost, then flashes the taskbar button if focus was still
// refused. Linux relies on Qt's raise and activation request plus the urgency hint.
#include "WindowAttention.h"

#include <QGuiApplication>
#include <QWindow>

#ifdef Q_OS_WIN
#include <windows.h>
#endif

void bringToFront(QWindow *window) {
    if (!window) return;
    if (window->windowStates() & Qt::WindowMinimized) window->setWindowStates(window->windowStates() & ~Qt::WindowMinimized);
    window->setVisible(true); window->raise(); window->requestActivate();
#ifdef Q_OS_WIN
    if (QGuiApplication::platformName() != QLatin1String("windows")) return;
    const auto hwnd = reinterpret_cast<HWND>(window->winId());
    if (IsIconic(hwnd)) ShowWindow(hwnd, SW_RESTORE);
    // Windows lets the thread that owns the foreground window hand it on, so
    // share its input state for the call.
    const HWND foreground = GetForegroundWindow();
    const DWORD self = GetCurrentThreadId(), owner = foreground ? GetWindowThreadProcessId(foreground, nullptr) : 0;
    const bool attached = owner && owner != self && AttachThreadInput(owner, self, TRUE);
    // Topmost and back again leaves the window above every normal window, even
    // when focus is refused.
    SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
    SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
    BringWindowToTop(hwnd);
    SetForegroundWindow(hwnd);
    if (attached) AttachThreadInput(owner, self, FALSE);
    if (GetForegroundWindow() != hwnd) {
        FLASHWINFO flash{sizeof(FLASHWINFO), hwnd, FLASHW_ALL | FLASHW_TIMERNOFG, 0, 0};
        FlashWindowEx(&flash);
    }
#else
    // The urgency hint, until the window is active again.
    window->alert(0);
#endif
}

void holdReminderTiming(bool) {}
