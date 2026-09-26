#pragma once

class QQuickWindow;

// Recolours the native title bar to the window's background colour and picks
// dark or light system controls to match, so it follows the Settings theme.
// TitleBar.mm on macOS, TitleBar.cpp elsewhere.
void matchTitleBar(QQuickWindow *window);
