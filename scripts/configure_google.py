#!/usr/bin/env python3
"""Configure this checkout's Google desktop registration once, without logging it."""
import argparse
import json
import os
from pathlib import Path
import re
import tempfile

ROOT = Path(__file__).resolve().parents[1]
LIMIT = 64 * 1024


def configure(source, destination):
    source = Path(source).expanduser()
    if source.is_symlink() or not source.is_file() or source.stat().st_size > LIMIT:
        raise ValueError("Use a Google installed-desktop client file smaller than 64 KiB.")
    with source.open("rb") as stream:
        contents = stream.read(LIMIT + 1)
    if len(contents) > LIMIT:
        raise ValueError("The client file exceeds 64 KiB.")
    try:
        value = json.loads(contents)
        installed = value["installed"]
        client = installed["client_id"]
        secret = installed["client_secret"]
        valid = "web" not in value and isinstance(client, str) and len(client) <= 1024 \
            and re.fullmatch(r"[A-Za-z0-9._-]+\.apps\.googleusercontent\.com", client) \
            and isinstance(secret, str) and 0 < len(secret) <= 4096 \
            and not any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in secret)
        if not valid:
            raise ValueError()
    except (KeyError, TypeError, ValueError):
        raise ValueError("Use the installed-desktop client downloaded from Google Cloud, not a web client or token file.") from None
    destination = Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    # Keep only application registration fields; never copy unrelated data.
    payload = json.dumps({"installed": {"client_id": client, "client_secret": secret}}) + "\n"
    fd, temporary = tempfile.mkstemp(prefix=".google-client-", dir=destination.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            stream.write(payload)
        os.replace(temporary, destination)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("client_file", type=Path)
    args = parser.parse_args()
    try:
        configure(args.client_file, ROOT / ".tools/google-oauth-client.json")
    except (ValueError, OSError) as failure:
        # OS exceptions can include user paths; print only a fixed setup message.
        parser.exit(1, (str(failure) if isinstance(failure, ValueError) else "Could not read or store the app registration.") + "\n")
    print("Google app registration configured locally. Rebuild/package to enable email-first sign-in.")


if __name__ == "__main__":
    main()
