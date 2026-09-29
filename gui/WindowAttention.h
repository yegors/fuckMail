#pragma once

class QWindow;

// Brings the main window forward for a meeting reminder: restores it when
// minimized, raises it above other apps' windows and asks for focus, with each
// platform's native help where Qt's request alone is refused.
// WindowAttention.mm on macOS, WindowAttention.cpp elsewhere.
void bringToFront(QWindow *window);
// Keeps the reminder countdown on time while a reminder is open. On macOS, App
// Nap otherwise delays the timers of a window hidden behind other windows.
void holdReminderTiming(bool active);
