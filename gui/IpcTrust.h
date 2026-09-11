#pragma once
#include <QJsonObject>
#include <QString>

// Validate the operating-system user boundary before sending a session secret.
QJsonObject readPrivateDiscovery(const QString &profile, QString &error);
bool verifyEnginePeer(qintptr descriptor, qint64 expectedPid, QString &error);
