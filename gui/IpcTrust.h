#pragma once
#include <QJsonObject>
#include <QList>
#include <QString>

// Validate the operating-system user boundary before sending a session secret.
//
// The engine writes `engine.json` (protocol, pid, endpoint, token) into the
// profile directory. Anyone who can read it can drive the engine, and anyone
// who can squat the endpoint name could collect the token, so clients check
// both sides before the token is sent. Used by MailBackend's main connection
// and by every GoogleOperation. Stateless; callable from any thread.

/// Reads the profile's engine discovery record, or returns an empty object.
///
/// An empty result with `error` empty means no engine is advertised yet.
/// With `error` set, the record exists but is not trustworthy: the profile
/// directory or file is not private to this user (another owner, POSIX modes
/// other than 0700/0600, a symlinked profile directory, a Windows reparse
/// point, or Windows ACL entries granting any other identity), is over
/// 16 KiB, or is not protocol 1 with a positive pid and a token. On POSIX a
/// symlinked `engine.json` is treated as absent.
QJsonObject readPrivateDiscovery(const QString &profile, QString &error);
/// True when the connected socket or named pipe `descriptor` belongs to
/// process `expectedPid` running as the same OS user as this process.
/// Call after connecting and before writing the token; sets `error` on false.
bool verifyEnginePeer(qintptr descriptor, qint64 expectedPid, QString &error);
/// The reason an engine refused this profile, when it names one of `launched`.
/// Reads `engine-refusal.json` (at most 4 KiB, not a symlink); returns "" when
/// it is missing or written by another process.
QString readStartupRefusal(const QString &profile, const QList<qint64> &launched);
