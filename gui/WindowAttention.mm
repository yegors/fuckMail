// bringToFront and holdReminderTiming for macOS: activates the app over the
// front app, orders the window front, bounces the Dock icon until the user
// looks, and holds an activity that keeps App Nap off the reminder countdown.
#include "WindowAttention.h"

#include <QGuiApplication>
#include <QWindow>

#import <AppKit/AppKit.h>

void bringToFront(QWindow *window) {
    if (!window) return;
    if (window->windowStates() & Qt::WindowMinimized) window->setWindowStates(window->windowStates() & ~Qt::WindowMinimized);
    window->setVisible(true); window->raise(); window->requestActivate();
    if (QGuiApplication::platformName() != QLatin1String("cocoa")) return;
    NSWindow *native = [reinterpret_cast<NSView *>(window->winId()) window];
    [NSApp unhide:nil];
    if (native.miniaturized) [native deminiaturize:nil];
    // Newer macOS versions may still refuse focus to a background app; ordering
    // the window front regardless at least puts it above the other windows.
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    [NSApp activateIgnoringOtherApps:YES];
#pragma clang diagnostic pop
    [native makeKeyAndOrderFront:nil];
    [native orderFrontRegardless];
    // Bounces until the app is active; does nothing if activation succeeded.
    [NSApp requestUserAttention:NSCriticalRequest];
}

void holdReminderTiming(bool active) {
    static CFTypeRef activity = nullptr;
    if (active == (activity != nullptr)) return;
    NSProcessInfo *process = [NSProcessInfo processInfo];
    if (active) {
        activity = CFBridgingRetain([process beginActivityWithOptions:NSActivityUserInitiatedAllowingIdleSystemSleep reason:@"Meeting reminder countdown"]);
    } else {
        [process endActivity:(__bridge id<NSObject>)activity];
        CFRelease(activity);
        activity = nullptr;
    }
}
