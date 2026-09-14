"""Offline tests for the local app-registration setup helper."""
import json
from pathlib import Path
import os
import stat
import tempfile
import unittest

from configure_google import configure


class ConfigureGoogleTest(unittest.TestCase):
    def test_minimal_private_configuration_and_failed_replacement_preserves_it(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, destination = root / "source.json", root / "local" / "client.json"
            installed = {"client_id": "synthetic.apps.googleusercontent.com", "client_secret": "synthetic"}
            source.write_text(json.dumps({"installed": {**installed, "token_uri": "https://example.invalid"}, "refresh_token": "never-copy"}))
            configure(source, destination)
            original = destination.read_bytes()
            self.assertEqual(json.loads(original), {"installed": installed})
            if os.name != "nt":
                self.assertEqual(stat.S_IMODE(destination.stat().st_mode), 0o600)
            for value in ({}, {"web": installed}, {"installed": []}, {"installed": {**installed, "client_id": "wrong"}},
                          {"installed": {**installed, "client_secret": ""}}, {"installed": {**installed, "client_secret": "bad\nsecret"}}):
                source.write_text(json.dumps(value))
                with self.assertRaises(ValueError):
                    configure(source, destination)
                self.assertEqual(destination.read_bytes(), original)
            source.write_bytes(b"x" * 65537)
            with self.assertRaises(ValueError):
                configure(source, destination)
            self.assertEqual(destination.read_bytes(), original)
            if os.name != "nt":
                link = root / "link.json"
                link.symlink_to(destination)
                with self.assertRaises(ValueError):
                    configure(link, destination)
                self.assertEqual(destination.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
