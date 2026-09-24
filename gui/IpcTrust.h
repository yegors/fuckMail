#pragma once
#include <QJsonObject>
#include <QList>
#include <QString>

// Validate the operating-system user boundary before sending a session secret.
QJsonObject readPrivateDiscovery(const QString &profile, QString &error);
bool verifyEnginePeer(qintptr descriptor, qint64 expectedPid, QString &error);
// The reason an engine refused this profile, when it names one of `launched`.
QString readStartupRefusal(const QString &profile, const QList<qint64> &launched);
