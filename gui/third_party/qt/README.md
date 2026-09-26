# Qt and FFmpeg license texts

Qt is not vendored here; the build uses an SDK installed by `scripts/dev.py bootstrap`.
Packages ship Qt's shared libraries, and Qt Multimedia's FFmpeg libraries, unmodified.
These texts travel with them because those licenses require it:

- `Qt-LGPL-3.0.txt`: the license Qt is used under;
- `Qt-GPL-3.0.txt`: the GPL text that LGPL-3.0 builds on and requires alongside it;
- `FFmpeg-LGPL-2.1.txt`: the license of the FFmpeg libraries Qt Multimedia uses.

fuckMail's own code is MIT-licensed; see [THIRD_PARTY_NOTICES.md](../../../THIRD_PARTY_NOTICES.md).
