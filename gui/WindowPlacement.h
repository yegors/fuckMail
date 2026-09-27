#pragma once
// Remembers where the main window was left: its normal (unmaximized) geometry
// and whether it was maximized, in <profile>/window.json. Restored through
// initial properties, so the window first appears in place. Also keeps the
// Mail list/reader divider, one width for every account (QML: windowLayout).
#include <QList>
#include <QObject>
#include <QRect>
#include <QTimer>
#include <QVariantMap>

class QWindow;

class WindowPlacement : public QObject {
    Q_OBJECT
    // The Mail list pane's width in logical pixels; 0 until the user drags it.
    Q_PROPERTY(qreal mailSplitWidth READ mailSplitWidth WRITE setMailSplitWidth NOTIFY mailSplitWidthChanged)
public:
    // `minimum` is the window's minimum size.
    WindowPlacement(const QString &profile, const QSize &minimum, QObject *parent = nullptr);
    // Initial properties for the window (x, y, width, height, visibility), or
    // none when nothing usable was saved or the saved place is off every screen.
    QVariantMap initialProperties() const;
    // Records the window's placement as it moves, resizes and maximizes.
    void track(QWindow *window);
    void save();
    qreal mailSplitWidth() const { return m_mailSplitWidth; }
    void setMailSplitWidth(qreal width);
signals:
    void mailSplitWidthChanged();
public:

    // Fits a saved normal rectangle to the screens' available areas: the title
    // area must show on a screen, and the size is capped to that screen.
    // An empty result means use the default placement.
    static QRect fit(const QRect &saved, const QList<QRect> &screens, const QSize &minimum);

private:
    void settle();
    QString m_path;
    QWindow *m_window = nullptr;
    QSize m_minimum;
    QRect m_normal;
    bool m_maximized = false;
    qreal m_mailSplitWidth = 0;
    QTimer m_settle;
};
