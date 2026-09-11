#include "IpcTrust.h"
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>

#ifdef Q_OS_WIN
#define NOMINMAX
#include <windows.h>
#include <aclapi.h>
#else
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#endif

namespace {
#ifdef Q_OS_WIN
QByteArray processUser(HANDLE process) {
    HANDLE token = nullptr;
    if (!OpenProcessToken(process, TOKEN_QUERY, &token)) return {};
    DWORD size = 0;
    GetTokenInformation(token, TokenUser, nullptr, 0, &size);
    QByteArray data(size, '\0');
    const bool ok = GetTokenInformation(token, TokenUser, data.data(), size, &size);
    CloseHandle(token);
    return ok ? data : QByteArray{};
}
bool privateWindowsPath(const QString &path, bool directory, QString &error) {
    const auto reject = [&error](const QString &reason) { error = reason; return false; };
    const auto wide = reinterpret_cast<LPCWSTR>(path.utf16());
    auto attributes = GetFileAttributesW(wide);
    if (attributes == INVALID_FILE_ATTRIBUTES) return reject("cannot inspect file attributes (Windows error " + QString::number(GetLastError()) + ")");
    if (attributes & FILE_ATTRIBUTE_REPARSE_POINT) return reject("reparse points are not allowed");
    if (bool(attributes & FILE_ATTRIBUTE_DIRECTORY) != directory) return reject("unexpected file type");
    auto current = processUser(GetCurrentProcess());
    if (current.isEmpty()) return reject("cannot inspect this process's user token");
    const auto sid = reinterpret_cast<TOKEN_USER *>(current.data())->User.Sid;
    PSID owner = nullptr; PACL dacl = nullptr; PSECURITY_DESCRIPTOR security = nullptr;
    const auto status = GetNamedSecurityInfoW(wide, SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION, &owner, nullptr, &dacl, nullptr, &security);
    if (status != ERROR_SUCCESS) return reject("cannot inspect ownership and permissions (Windows error " + QString::number(status) + ")");
    bool ok = owner && EqualSid(owner, sid) && dacl;
    if (!owner || !EqualSid(owner, sid)) error = "owner is not the current process user";
    else if (!dacl) error = "permission list is absent";
    if (ok) for (DWORD index = 0; index < dacl->AceCount; ++index) {
        void *raw = nullptr;
        if (!GetAce(dacl, index, &raw)) { ok = false; error = "cannot inspect a permission entry"; break; }
        const auto header = static_cast<ACE_HEADER *>(raw);
        if (header->AceType == ACCESS_ALLOWED_ACE_TYPE) {
            const auto ace = static_cast<ACCESS_ALLOWED_ACE *>(raw);
            if (ace->Mask && !EqualSid(&ace->SidStart, sid)) { ok = false; error = "a permission entry grants another identity access"; break; }
        } else if (header->AceType != ACCESS_DENIED_ACE_TYPE) { ok = false; error = "unsupported permission entry type"; break; }
    }
    LocalFree(security); return ok;
}
#else
bool privateStat(const struct stat &info, bool directory) {
    return info.st_uid == geteuid() && (directory ? S_ISDIR(info.st_mode) : S_ISREG(info.st_mode))
        && (info.st_mode & 0777) == (directory ? 0700 : 0600);
}
#endif
}

QJsonObject readPrivateDiscovery(const QString &profile, QString &error) {
    QFile discovery;
#ifdef Q_OS_WIN
    const auto path = QDir(profile).filePath("engine.json");
    if (!QFileInfo::exists(path)) return {};
    QString reason;
    if (!privateWindowsPath(profile, true, reason)) { error = "The Windows profile is not private: " + reason + "."; return {}; }
    if (!privateWindowsPath(path, false, reason)) { error = "The Windows engine discovery file is not private: " + reason + "."; return {}; }
    discovery.setFileName(path);
    if (!discovery.open(QIODevice::ReadOnly)) { error = "Cannot read the private engine discovery file."; return {}; }
#else
    const auto directory = ::open(QFile::encodeName(profile).constData(), O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
    if (directory < 0) {
        if (QFileInfo::exists(profile)) error = "The local profile must be a private directory, not a symbolic link.";
        return {};
    }
    struct stat info{};
    if (fstat(directory, &info) != 0 || !privateStat(info, true)) {
        ::close(directory); error = "The local profile must belong to this user and have mode 0700."; return {};
    }
    const auto file = openat(directory, "engine.json", O_RDONLY | O_NOFOLLOW | O_CLOEXEC);
    ::close(directory);
    if (file < 0) return {};
    if (fstat(file, &info) != 0 || !privateStat(info, false)) {
        ::close(file); error = "The engine discovery file must belong to this user and have mode 0600."; return {};
    }
    if (!discovery.open(file, QIODevice::ReadOnly, QFileDevice::AutoCloseHandle)) {
        ::close(file); error = "Cannot read the private engine discovery file."; return {};
    }
#endif
    if (discovery.size() > 16384) { error = "The engine discovery file is too large."; return {}; }
    const auto data = QJsonDocument::fromJson(discovery.readAll()).object();
    if (data["protocol"].toInt() != 1 || data["pid"].toInteger() <= 0 || data["token"].toString().isEmpty()) {
        error = "The engine discovery file is incompatible."; return {};
    }
    return data;
}

bool verifyEnginePeer(qintptr descriptor, qint64 expectedPid, QString &error) {
    bool ok = false;
#ifdef Q_OS_WIN
    ULONG pid = 0;
    if (GetNamedPipeServerProcessId(reinterpret_cast<HANDLE>(descriptor), &pid) && pid == expectedPid) {
        auto process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if (process) {
            auto peer = processUser(process), current = processUser(GetCurrentProcess());
            CloseHandle(process);
            ok = !peer.isEmpty() && !current.isEmpty() && EqualSid(reinterpret_cast<TOKEN_USER *>(peer.data())->User.Sid, reinterpret_cast<TOKEN_USER *>(current.data())->User.Sid);
        }
    }
#elif defined(Q_OS_MACOS)
    uid_t user = 0; gid_t group = 0; pid_t pid = 0; socklen_t length = sizeof(pid);
    ok = getpeereid(static_cast<int>(descriptor), &user, &group) == 0 && user == geteuid()
        && getsockopt(static_cast<int>(descriptor), SOL_LOCAL, LOCAL_PEERPID, &pid, &length) == 0 && pid == expectedPid;
#else
    struct ucred peer{}; socklen_t length = sizeof(peer);
    ok = getsockopt(static_cast<int>(descriptor), SOL_SOCKET, SO_PEERCRED, &peer, &length) == 0 && peer.uid == geteuid() && peer.pid == expectedPid;
#endif
    if (!ok) error = "The local engine process does not match the private discovery record and operating-system user.";
    return ok;
}
