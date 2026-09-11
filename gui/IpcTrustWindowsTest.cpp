#include "IpcTrust.h"
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QJsonDocument>
#include <QLocalServer>
#include <QLocalSocket>
#include <QTemporaryDir>
#include <memory>

#define NOMINMAX
#include <windows.h>
#include <sddl.h>

namespace {
struct LocalDescriptor {
    PSECURITY_DESCRIPTOR value = nullptr;
    ~LocalDescriptor() { if (value) LocalFree(value); }
    bool set(const QString &sddl) {
        return ConvertStringSecurityDescriptorToSecurityDescriptorW(
            reinterpret_cast<LPCWSTR>(sddl.utf16()), SDDL_REVISION_1, &value, nullptr);
    }
    SECURITY_ATTRIBUTES attributes() { return {sizeof(SECURITY_ATTRIBUTES), value, FALSE}; }
};
QString currentSid() {
    HANDLE token = nullptr;
    if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token)) return {};
    DWORD length = 0;
    GetTokenInformation(token, TokenUser, nullptr, 0, &length);
    QByteArray data(length, '\0');
    const auto ok = GetTokenInformation(token, TokenUser, data.data(), length, &length);
    CloseHandle(token);
    if (!ok) return {};
    LPWSTR sid = nullptr;
    if (!ConvertSidToStringSidW(reinterpret_cast<TOKEN_USER *>(data.data())->User.Sid, &sid)) return {};
    const auto result = QString::fromWCharArray(sid);
    LocalFree(sid);
    return result;
}
bool writePrivate(const QString &path, const QByteArray &bytes, SECURITY_ATTRIBUTES &attributes) {
    auto file = CreateFileW(reinterpret_cast<LPCWSTR>(path.utf16()), GENERIC_WRITE,
        FILE_SHARE_READ, &attributes, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) return false;
    DWORD written = 0;
    const auto ok = WriteFile(file, bytes.constData(), DWORD(bytes.size()), &written, nullptr);
    CloseHandle(file);
    return ok && written == bytes.size();
}
}

#define CHECK(condition) do { if (!(condition)) { qCritical() << "Windows IPC trust check failed at line" << __LINE__ << ":" << error; return 1; } } while (false)

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    QString error;
    const auto sid = currentSid();
    CHECK(!sid.isEmpty());
    QTemporaryDir temp;
    CHECK(temp.isValid());
    LocalDescriptor privateSecurity;
    CHECK(privateSecurity.set("O:" + sid + "D:P(A;OICI;GA;;;" + sid + ")"));
    auto attributes = privateSecurity.attributes();
    const auto profile = QDir(temp.path()).filePath("profile");
    CHECK(CreateDirectoryW(reinterpret_cast<LPCWSTR>(profile.utf16()), &attributes));
    const auto discovery = QDir(profile).filePath("engine.json");
    const auto document = QJsonDocument(QJsonObject{{"protocol", 1}, {"pid", QCoreApplication::applicationPid()},
        {"endpoint", "fixture"}, {"token", "synthetic-test-token"}}).toJson();
    CHECK(writePrivate(discovery, document, attributes));
    CHECK(!readPrivateDiscovery(profile, error).isEmpty());
    CHECK(error.isEmpty());

    LocalDescriptor sharedSecurity;
    CHECK(sharedSecurity.set("O:" + sid + "D:P(A;;GA;;;" + sid + ")(A;;GR;;;WD)"));
    CHECK(SetFileSecurityW(reinterpret_cast<LPCWSTR>(discovery.utf16()),
        DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION, sharedSecurity.value));
    CHECK(readPrivateDiscovery(profile, error).isEmpty());
    CHECK(error.contains("grants another identity access"));
    CHECK(SetFileSecurityW(reinterpret_cast<LPCWSTR>(discovery.utf16()),
        DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION, privateSecurity.value));
    error.clear();
    CHECK(!readPrivateDiscovery(profile, error).isEmpty());

    QLocalServer server;
    server.setSocketOptions(QLocalServer::UserAccessOption);
    CHECK(server.listen("fuckmail-trust-test-" + QString::number(QCoreApplication::applicationPid())));
    QLocalSocket client;
    client.connectToServer(server.serverName());
    CHECK(client.waitForConnected(2000));
    CHECK(verifyEnginePeer(client.socketDescriptor(), QCoreApplication::applicationPid(), error));
    CHECK(!verifyEnginePeer(client.socketDescriptor(), QCoreApplication::applicationPid() + 1, error));
    CHECK(error.contains("does not match"));
    client.abort(); server.close();
    qInfo() << "Windows IPC trust checks passed: explicit owner-private discovery, broad ACL rejection, actual named-pipe server PID/user validation.";
    return 0;
}
