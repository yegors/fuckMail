// IpcTrust test for macOS and Linux (gui-ipc-trust): discovery file and profile
// modes, symlink rejection, and matching the connected peer's user and PID.
// Windows builds use IpcTrustWindowsTest.cpp instead.
#include "IpcTrust.h"
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QFile>
#include <QLocalServer>
#include <QLocalSocket>
#include <QTemporaryDir>
#ifndef Q_OS_WIN
#include <unistd.h>
#endif

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
#ifndef Q_OS_WIN
    QTemporaryDir fixture("/tmp/fm-ipc-XXXXXX");
    if (!fixture.isValid()) return 1;
    auto directory = fixture.path();
    auto path = QDir(directory).filePath("engine.json");
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly)) return 2;
    file.write("{\"protocol\":1,\"pid\":1,\"token\":\"synthetic-test-token\",\"endpoint\":\"unused\"}");
    file.close();
    file.setPermissions(QFile::ReadOwner | QFile::WriteOwner);
    QString error;
    if (readPrivateDiscovery(directory, error).isEmpty()) return 3;
    file.setPermissions(QFile::ReadOwner | QFile::WriteOwner | QFile::ReadOther);
    if (!readPrivateDiscovery(directory, error).isEmpty()) return 4;
    file.setPermissions(QFile::ReadOwner | QFile::WriteOwner);
    QFile::setPermissions(directory, QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner | QFile::ReadOther | QFile::ExeOther);
    if (!readPrivateDiscovery(directory, error).isEmpty()) return 5;
    QFile::setPermissions(directory, QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner);
    file.rename(QDir(directory).filePath("original.json"));
    if (!QFile::link(QDir(directory).filePath("original.json"), path)) return 6;
    if (!readPrivateDiscovery(directory, error).isEmpty()) return 7;
    QLocalServer server;
    if (!server.listen(QDir(directory).filePath("peer.sock"))) { qCritical() << server.errorString(); return 8; }
    QLocalSocket client;
    client.connectToServer(server.fullServerName());
    if (!client.waitForConnected(1000) || !server.waitForNewConnection(1000)) return 9;
    auto peer = server.nextPendingConnection();
    if (!verifyEnginePeer(client.socketDescriptor(), getpid(), error)) return 10;
    if (verifyEnginePeer(client.socketDescriptor(), getpid() + 100000, error)) return 11;
    delete peer;
    qInfo() << "GUI IPC trust checks passed: owner modes, symlink rejection, native same-user/process peer match and mismatched PID rejection.";
#else
    // Real Windows ACL/peer validation runs in gui-integration against fm-engine.
    qInfo() << "Windows GUI IPC trust checks are exercised by gui-integration.";
#endif
    return 0;
}
