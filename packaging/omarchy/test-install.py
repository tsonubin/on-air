#!/usr/bin/env python3
"""Exercise the installer without compiling or installing system packages."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


INSTALLER = Path(__file__).resolve().with_name("install.sh")
MOCK = r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import subprocess
import sys

command = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["INSTALL_TEST_LOG"], "a") as log:
    log.write(json.dumps([command, *args]) + "\n")
package = os.environ["INSTALL_TEST_PACKAGE"]
if command == "makepkg":
    if "--packagelist" in args:
        print(package)
    else:
        if os.environ.get("INSTALL_TEST_BUILD_FAIL"):
            sys.exit(1)
        Path(package).touch()
        if "-si" in args or "--install" in args:
            sys.exit(subprocess.call(["sudo", "-k", "pacman", "-U", package]))
elif command == "sudo":
    if "-k" in args:
        print("sudo: forced reauthentication; cached credentials ignored", file=sys.stderr)
        sys.exit(1)
    if os.environ.get("INSTALL_TEST_AUTH_FAIL"):
        print("sudo: authentication failed", file=sys.stderr)
        sys.exit(1)
'''


class InstallerTest(unittest.TestCase):
    def run_installer(self, **extra_env):
        temporary = tempfile.TemporaryDirectory(prefix="on-air-install-test-")
        self.addCleanup(temporary.cleanup)
        work = Path(temporary.name)
        mock = work / "mock"
        mock.write_text(MOCK)
        mock.chmod(0o755)
        bin_dir = work / "bin"
        bin_dir.mkdir()
        for command in ("makepkg", "sudo", "cargo", "git", "systemctl", "cat"):
            (bin_dir / command).symlink_to(mock)
        log = work / "commands.jsonl"
        # Package output may be outside the build directory (PKGDEST), with spaces.
        package = work / "package output" / "on-air-0.2.0-1-x86_64.pkg.tar.zst"
        package.parent.mkdir()
        result = subprocess.run(
            ["sh", str(INSTALLER)],
            env={
                **os.environ,
                "PATH": f"{bin_dir}:{os.environ['PATH']}",
                "XDG_CACHE_HOME": str(work / "cache"),
                "INSTALL_TEST_LOG": str(log),
                "INSTALL_TEST_PACKAGE": str(package),
                **extra_env,
            },
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
        )
        calls = [json.loads(line) for line in log.read_text().splitlines()]
        return result, calls, package

    def test_install_preserves_sudo_authentication(self):
        result, calls, package = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(["sudo", "pacman", "-U", "--", str(package)], calls)
        self.assertIn(["systemctl", "--user", "enable", "--now", "on-air-core"], calls)

    def test_authentication_failure_keeps_package_and_stops_before_service(self):
        result, calls, package = self.run_installer(INSTALL_TEST_AUTH_FAIL="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(package.is_file())
        self.assertIn(str(package), result.stderr)
        self.assertFalse(any(call[0] == "systemctl" for call in calls))

    def test_build_failure_does_not_attempt_installation(self):
        result, calls, _ = self.run_installer(INSTALL_TEST_BUILD_FAIL="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(call[0] in ("sudo", "systemctl") for call in calls))


if __name__ == "__main__":
    unittest.main()
