"""Run pinned aqt with the Qt 6.11.2 Windows metadata layout correction.

Upstream aqt issue: https://github.com/miurahr/aqtinstall/issues/1007
This changes only the selected kit's repository suffix. Upstream metadata/archive
checksum checks, TLS downloads, extraction and path relocation remain enabled.
No installed aqt file is modified. Remove this adapter after aqt supports the layout.
"""
from importlib.metadata import version
import sys

from aqt import main
from aqt.archives import QtArchives

if version("aqtinstall") != "3.3.0":
    raise RuntimeError("Review the Qt metadata adapter before changing aqtinstall")

original_arch_ext = QtArchives._arch_ext


def selected_arch_ext(archives):
    if (archives.os_name == "windows" and archives.target == "desktop"
            and str(archives.version) == "6.11.2" and archives.arch == "win64_msvc2022_64"):
        return "_msvc2022_64"
    return original_arch_ext(archives)


QtArchives._arch_ext = selected_arch_ext

if __name__ == "__main__":
    sys.exit(main())
