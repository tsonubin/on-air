"""Installer updates must preserve local edits and avoid duplicate manifests."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class PluginInstall(unittest.TestCase):
    def test_install_update_backup_and_explicit_enable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "bin").mkdir()
            fake = root / "bin/omarchy"
            fake.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$ON_AIR_PLUGIN_TEST_LOG"\n')
            fake.chmod(0o755)
            log = root / "commands"
            env = {**os.environ, "XDG_CONFIG_HOME": str(root / "config"),
                   "XDG_STATE_HOME": str(root / "state"), "ON_AIR_PLUGIN_TEST_LOG": str(log),
                   "PATH": str(root / "bin") + os.pathsep + os.environ["PATH"]}
            script = Path(__file__).with_name("install-plugin.sh")

            def run(*args):
                return subprocess.run(["sh", str(script), *args], env=env, text=True, capture_output=True)

            first = run()
            self.assertEqual(first.returncode, 0, first.stderr)
            destination = root / "config/omarchy/plugins/on-air.patchbay"
            self.assertEqual(json.loads((destination / "manifest.json").read_text())["id"], "on-air.patchbay")
            self.assertTrue((destination / "Widget.qml").is_file())
            self.assertFalse(any(p.is_symlink() for p in destination.rglob("*")))
            edit = destination / "local-note"
            edit.write_text("keep my changes")
            self.assertNotEqual(run().returncode, 0)
            self.assertEqual(edit.read_text(), "keep my changes")
            update = run("--replace", "--enable")
            self.assertEqual(update.returncode, 0, update.stderr)
            self.assertFalse(edit.exists())
            backups = list((root / "state/on-air/plugin-backups").glob("*"))
            self.assertEqual(len(backups), 1)
            self.assertEqual((backups[0] / "local-note").read_text(), "keep my changes")
            self.assertEqual(len(list((root / "config/omarchy/plugins").glob("*/manifest.json"))), 1)
            self.assertIn("plugin validate", log.read_text())
            self.assertIn("plugin enable on-air.patchbay", log.read_text())
            self.assertIn("bar move on-air.patchbay --after omarchy.clock", log.read_text())
            self.assertNotEqual(run("--unknown").returncode, 0)


if __name__ == "__main__":
    unittest.main()
