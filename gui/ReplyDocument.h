#pragma once
#include <QObject>
#include <QPointer>
#include <QQuickItem>
#include <QQuickTextDocument>
#include <QtQml/qqmlregistration.h>

// A human-authored, limited-format editor. Received messages never enter it.
class ReplyDocument : public QObject {
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(QQuickTextDocument *textDocument READ textDocument WRITE setTextDocument NOTIFY documentChanged)
    Q_PROPERTY(QQuickItem *editor READ editor WRITE setEditor NOTIFY documentChanged)
    Q_PROPERTY(QString plainText READ plainText NOTIFY contentChanged)
    Q_PROPERTY(QString html READ html NOTIFY contentChanged)
public:
    explicit ReplyDocument(QObject *parent = nullptr) : QObject(parent) {}
    QQuickTextDocument *textDocument() const { return m_document; }
    QQuickItem *editor() const { return m_editor; }
    void setTextDocument(QQuickTextDocument *document);
    void setEditor(QQuickItem *editor);
    QString plainText() const;
    QString html() const;
    Q_INVOKABLE void setIdentity(const QString &identity);
    Q_INVOKABLE void load(const QString &body, const QString &html);
    Q_INVOKABLE void format(const QString &kind);
    Q_INVOKABLE void pastePlain();
    Q_INVOKABLE void undo();
    Q_INVOKABLE void redo();
    static QString limitedHtml(const QString &html);
signals:
    void documentChanged();
    void contentChanged();
protected:
    bool eventFilter(QObject *watched, QEvent *event) override;
private:
    QPointer<QQuickTextDocument> m_document;
    QPointer<QQuickItem> m_editor;
    QString m_identity;
    bool m_loading = false;
};
