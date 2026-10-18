#!/usr/bin/env python3
"""Exercise packaging/install.sh without building or installing anything.

The desktop installer drives external tools (cargo, pnpm, makepkg, sudo,
flatpak, ...). This suite runs it with PATH set to a private bin directory:
real coreutils are symlinked in, everything that would build, authenticate,
or reach the network is a mock that records its invocation as JSONL. A
throwaway fixture tree stands in for the checkout (symlinks to the real
inputs the installer reads) so every write lands in the sandbox and can be
asserted on failure.

Pinned behaviour that protects a user:
  * a failure stops before installation: no pacman/yay/pkexec/flatpak call,
    and nothing written outside the staging area;
  * success and authentication failure clean up the staging directory, while
    the built package is preserved (packaging/dist) with the manual install
    command when authentication fails;
  * a makepkg failure deliberately leaves the staging directory behind (the
    script has no cleanup trap) and nothing else.

Needs only python3. No network, no root, no Arch or Flatpak toolchain.
"""

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

REPO = Path(__file__).resolve().parent.parent
APP_ID = "com.shay.on-air"
VERSION = json.loads((REPO / "apps/desktop/src-tauri/tauri.conf.json").read_text())["version"]

MOCK = r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import shutil
import sys

command = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["INSTALL_TEST_LOG"], "a") as log:
    log.write(json.dumps([command, *args]) + "\n")


def flag(name):
    return os.environ.get(name)


if command == "cargo":
    if flag("INSTALL_TEST_BUILD_FAIL"):
        sys.exit(1)
    if not flag("INSTALL_TEST_NO_BINARY"):
        binary = Path(os.environ["INSTALL_TEST_BINARY"])
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_text("#!/bin/sh\nexit 0\n")
        binary.chmod(0o755)
elif command == "pnpm":
    if flag("INSTALL_TEST_PNPM_FAIL"):
        sys.exit(1)
elif command == "pkg-config":
    if flag("INSTALL_TEST_NO_WEBKIT"):
        sys.exit(1)
elif command == "makepkg":
    capture = os.environ.get("INSTALL_TEST_CAPTURE")
    if capture:
        capture = Path(capture)
        capture.mkdir(parents=True, exist_ok=True)
        shutil.copyfile("PKGBUILD", capture / "PKGBUILD")
        (capture / "staged.txt").write_text(
            "\n".join(sorted(p.name for p in Path(".").iterdir()))
        )
    if flag("INSTALL_TEST_MAKEPKG_FAIL"):
        sys.exit(1)
    Path("on-air-desktop-1-1-test.pkg.tar.zst").touch()
elif command == "sudo":
    if args[:2] == ["-n", "true"]:
        sys.exit(1 if flag("INSTALL_TEST_NO_SUDO") else 0)
    # sudo itself works, but the package install it is asked to run fails.
    if "pacman" in args and flag("INSTALL_TEST_PACMAN_FAIL"):
        sys.exit(1)
elif command == "pkexec":
    if flag("INSTALL_TEST_PKEXEC_FAIL"):
        sys.exit(1)
elif command == "pgrep":
    sys.exit(0 if flag("INSTALL_TEST_POLKIT_AGENT") else 1)
elif command == "pacman":
    if args and args[0] == "-Q":
        sys.exit(0 if flag("INSTALL_TEST_PACMAN_Q") else 1)
elif command == "git":
    if "clone" in args:
        if flag("INSTALL_TEST_GIT_FAIL"):
            sys.exit(1)
        shared = Path(args[-1]) / "libappindicator"
        shared.mkdir(parents=True, exist_ok=True)
        (shared / "libappindicator-gtk3-12.10.json").write_text("{}\n")
    elif "rev-parse" in args:
        print("1" * 40)
'''

# Everything the installer may invoke that would build, authenticate,
# install, or reach the network. yay is opt-in: with yay on PATH the
# installer prefers it over sudo pacman.
DEFAULT_MOCKS = (
    "cargo", "pnpm", "npm", "pkg-config", "makepkg", "sudo", "pkexec",
    "pgrep", "pacman", "git", "flatpak", "flatpak-builder",
)

# Real tools the installer needs, resolved to their real paths so they keep
# working even where /etc is shadowed inside a mount namespace.
REAL_TOOLS = (
    "bash", "sh", "mount", "sed", "awk", "sha256sum", "head", "find",
    "install", "mkdir", "cp", "rm", "cat", "basename", "dirname", "uname",
    "mktemp", "python3",
)

# Inputs packaging/install.sh reads from the checkout; symlinked into the
# fixture so a missing real input still fails the suite.
FIXTURE_INPUTS = (
    "packaging/install.sh",
    "LICENSE",
    "apps/desktop/src-tauri/tauri.conf.json",
    "apps/desktop/src-tauri/icons",
    "packaging/flatpak/com.shay.on-air.yml",
    "packaging/flatpak/com.shay.on-air.desktop",
)

USER_FILES = {
    ".local/bin/on-air-desktop",
    ".local/share/applications/com.shay.on-air.desktop",
    ".local/share/icons/hicolor/32x32/apps/com.shay.on-air.png",
    ".local/share/icons/hicolor/128x128/apps/com.shay.on-air.png",
    ".local/share/icons/hicolor/256x256/apps/com.shay.on-air.png",
    ".local/share/icons/hicolor/256x256/apps/on-air-desktop.png",
    ".local/share/icons/hicolor/512x512/apps/com.shay.on-air.png",
}

STAGED_FILES = {
    "PKGBUILD", "on-air-desktop", "LICENSE", "com.shay.on-air.desktop",
    "icon-32.png", "icon-128.png", "icon-256.png", "icon-512.png",
}


def tree(root):
    entries = set()
    for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
        for name in dirnames + filenames:
            entries.add(os.path.relpath(os.path.join(dirpath, name), root))
    return entries


def with_parents(paths):
    out = set(paths)
    for path in paths:
        parts = Path(path).parts
        for i in range(1, len(parts)):
            out.add(str(Path(*parts[:i])))
    return out


def install_calls(calls):
    """Calls that install software or bring in remote content."""
    out = []
    for call in calls:
        if call[0] in ("pacman", "yay", "pkexec", "flatpak", "flatpak-builder"):
            out.append(call)
        elif call[0] == "sudo" and call[1:2] != ["-n"]:
            out.append(call)
    return out


def pkgbuild_list(text, key):
    block = re.search(r"^" + key + r"=\((.*?)\)", text, re.S | re.M).group(1)
    return [line.strip().strip("'") for line in block.splitlines() if line.strip()]


def pkgbuild_sums(text):
    block = re.search(r"^sha256sums=\((.*?)\)", text, re.S | re.M).group(1)
    return re.findall(r"'([0-9a-f]{64})'", block)


def have_unshare():
    unshare = shutil.which("unshare")
    if not unshare:
        return False
    try:
        subprocess.run(
            [unshare, "-Ur", "-m", "true"],
            check=True, stdin=subprocess.DEVNULL,
            capture_output=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return False
    return True


UNSHARE = shutil.which("unshare")
HAVE_UNSHARE = have_unshare()


class Run:
    """One installer invocation: exit status, recorded calls, tree deltas."""

    def __init__(self, result, calls, before, after):
        self.result = result
        self.calls = calls
        self.before = before
        self.after = after

    def added(self, which):
        return self.after[which] - self.before[which]

    def called(self, command):
        return [call for call in self.calls if call[0] == command]


class DesktopInstallerTest(unittest.TestCase):
    def setUp(self):
        base = tempfile.TemporaryDirectory(prefix="on-air-desktop-install-test-")
        self.addCleanup(base.cleanup)
        self.work = Path(base.name)
        self.home = self.work / "home"
        self.home.mkdir()
        self.tmpdir = self.work / "tmp"
        self.tmpdir.mkdir()
        self.log = self.work / "commands.jsonl"
        self.log.touch()
        self.capture = self.work / "capture"
        self.root = self.work / "checkout"
        self.bin_runs = 0
        self.mock = self.work / "mock"
        self.mock.write_text(MOCK)
        self.mock.chmod(0o755)
        for rel in FIXTURE_INPUTS:
            source = REPO / rel
            self.assertTrue(source.exists(), f"missing installer input {rel}")
            dest = self.root / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.symlink_to(source)

    def make_bin(self, mocks):
        bin_dir = self.work / f"bin-{self.bin_runs}"
        self.bin_runs += 1
        bin_dir.mkdir()
        for tool in REAL_TOOLS:
            target = sys.executable if tool == "python3" else shutil.which(tool)
            self.assertTrue(target, f"{tool} is required to run the tests")
            (bin_dir / tool).symlink_to(os.path.realpath(target))
        for name in mocks:
            (bin_dir / name).symlink_to(self.mock)
        return bin_dir

    def run_installer(self, args=("auto",), mocks=DEFAULT_MOCKS, env=None,
                      arch_release=None):
        bin_dir = self.make_bin(mocks)
        installer = self.root / "packaging/install.sh"
        self.log.write_text("")
        run_env = {
            **os.environ,
            "HOME": str(self.home),
            "TMPDIR": str(self.tmpdir),
            "PATH": str(bin_dir),
            "INSTALL_TEST_LOG": str(self.log),
            "INSTALL_TEST_CAPTURE": str(self.capture),
            "INSTALL_TEST_BINARY": str(self.root / "target/release/on-air-desktop"),
            **(env or {}),
        }
        if arch_release is None:
            command = [str(bin_dir / "bash"), str(installer), *args]
        else:
            self.assertTrue(HAVE_UNSHARE, "needs user namespaces to fake /etc")
            fake_etc = self.work / ("fake-etc-arch" if arch_release else "fake-etc")
            fake_etc.mkdir(exist_ok=True)
            if arch_release:
                (fake_etc / "arch-release").touch()
            run_env["FAKE_ETC"] = str(fake_etc)
            command = [
                UNSHARE, "-Ur", "-m", "sh", "-c",
                'mount --bind "$FAKE_ETC" /etc && exec "$@"', "on-air-test",
                str(bin_dir / "bash"), str(installer), *args,
            ]
        names = ("root", "home", "tmpdir")
        before = {name: tree(getattr(self, name)) for name in names}
        result = subprocess.run(
            command, env=run_env, stdin=subprocess.DEVNULL,
            capture_output=True, text=True, cwd=str(self.work),
        )
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        after = {name: tree(getattr(self, name)) for name in names}
        return Run(result, calls, before, after)

    def assertNoInstallation(self, run):
        self.assertEqual(install_calls(run.calls), [], run.calls)
        self.assertEqual(run.called("makepkg"), [], run.calls)

    def assertNothingWritten(self, run):
        self.assertEqual(run.added("root"), set(), run.result.stdout + run.result.stderr)
        self.assertEqual(run.added("home"), set(), run.result.stdout + run.result.stderr)
        self.assertEqual(run.added("tmpdir"), set(), run.result.stdout + run.result.stderr)

    # -- mode selection ----------------------------------------------------

    def test_auto_without_package_managers_installs_user(self):
        mocks = tuple(m for m in DEFAULT_MOCKS if m not in ("makepkg", "flatpak", "flatpak-builder"))
        run = self.run_installer(("auto",), mocks=mocks)
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        self.assertTrue((self.home / ".local/bin/on-air-desktop").is_file())
        self.assertEqual(run.called("makepkg"), [])
        self.assertEqual(run.called("flatpak"), [])

    def test_auto_prefers_flatpak_over_user(self):
        mocks = tuple(m for m in DEFAULT_MOCKS if m != "makepkg")
        run = self.run_installer(("auto",), mocks=mocks)
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        self.assertNotEqual(run.called("flatpak"), [])
        self.assertEqual(run.added("home"), set())

    def assert_auto_arch(self, arch_release):
        if HAVE_UNSHARE:
            run = self.run_installer(("auto",), arch_release=arch_release)
        else:
            # Without user namespaces only the host's real /etc can decide.
            if Path("/etc/arch-release").is_file() != arch_release:
                self.skipTest("needs user namespaces to fake /etc/arch-release")
            run = self.run_installer(("auto",))
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        self.assertEqual(bool(run.called("makepkg")), arch_release, run.calls)
        self.assertEqual(bool(run.called("flatpak")), not arch_release, run.calls)

    def test_auto_uses_yay_when_arch_and_makepkg(self):
        self.assert_auto_arch(arch_release=True)

    def test_auto_ignores_makepkg_off_arch(self):
        self.assert_auto_arch(arch_release=False)

    def test_method_aliases_route_like_the_canonical_names(self):
        for alias, marker in (("arch", "makepkg"), ("pacman", "makepkg"), ("local", "bin")):
            with self.subTest(alias=alias):
                run = self.run_installer((alias,))
                self.assertEqual(run.result.returncode, 0, run.result.stderr)
                if marker == "makepkg":
                    self.assertNotEqual(run.called("makepkg"), [], run.calls)
                else:
                    self.assertTrue((self.home / ".local/bin/on-air-desktop").is_file())

    # -- argument validation ------------------------------------------------

    def test_unknown_method_fails_without_touching_anything(self):
        run = self.run_installer(("--bogus",))
        self.assertEqual(run.result.returncode, 1)
        self.assertIn("unknown method", run.result.stderr)
        self.assertEqual(run.calls, [])
        self.assertNothingWritten(run)

    def test_help_prints_usage_without_touching_anything(self):
        run = self.run_installer(("--help",))
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        self.assertIn("Install the on-air desktop app", run.result.stdout)
        self.assertEqual(run.calls, [])
        self.assertNothingWritten(run)

    # -- missing dependencies ----------------------------------------------

    def test_missing_cargo_stops_before_building(self):
        mocks = tuple(m for m in DEFAULT_MOCKS if m != "cargo")
        run = self.run_installer(("user",), mocks=mocks)
        self.assertNotEqual(run.result.returncode, 0)
        self.assertIn("cargo is required", run.result.stderr)
        self.assertNoInstallation(run)
        self.assertNothingWritten(run)

    def test_missing_pnpm_and_npm_stop_before_building(self):
        mocks = tuple(m for m in DEFAULT_MOCKS if m not in ("pnpm", "npm"))
        run = self.run_installer(("user",), mocks=mocks)
        self.assertNotEqual(run.result.returncode, 0)
        self.assertIn("pnpm or npm is required", run.result.stderr)
        self.assertNoInstallation(run)
        self.assertNothingWritten(run)

    def test_missing_webkit_dev_files_stop_before_building(self):
        run = self.run_installer(("user",), env={"INSTALL_TEST_NO_WEBKIT": "1"})
        self.assertNotEqual(run.result.returncode, 0)
        self.assertIn("webkit2gtk-4.1 and gtk3", run.result.stderr)
        self.assertNoInstallation(run)
        self.assertNothingWritten(run)

    def test_yay_mode_requires_makepkg(self):
        mocks = tuple(m for m in DEFAULT_MOCKS if m != "makepkg")
        run = self.run_installer(("yay",), mocks=mocks)
        self.assertNotEqual(run.result.returncode, 0)
        self.assertIn("makepkg is required", run.result.stderr)
        self.assertNoInstallation(run)
        self.assertNothingWritten(run)

    def test_flatpak_mode_requires_flatpak(self):
        run = self.run_installer(("flatpak",), mocks=tuple(
            m for m in DEFAULT_MOCKS if m not in ("flatpak", "flatpak-builder")))
        self.assertNotEqual(run.result.returncode, 0)
        self.assertIn("flatpak is required", run.result.stderr)
        self.assertNoInstallation(run)
        self.assertNothingWritten(run)

    # -- build failures stop before installation ----------------------------

    def test_build_failure_stops_before_any_installation(self):
        # A stale release binary from an earlier build must not slip through
        # when the build itself fails: the failure has to stop the script.
        stale = self.root / "target/release/on-air-desktop"
        stale.parent.mkdir(parents=True)
        stale.write_text("#!/bin/sh\nexit 0\n")
        stale.chmod(0o755)
        run = self.run_installer(("yay",), env={"INSTALL_TEST_BUILD_FAIL": "1"})
        self.assertNotEqual(run.result.returncode, 0)
        self.assertNoInstallation(run)
        self.assertNothingWritten(run)

    def test_frontend_build_failure_stops_before_any_installation(self):
        run = self.run_installer(("user",), env={"INSTALL_TEST_PNPM_FAIL": "1"})
        self.assertNotEqual(run.result.returncode, 0)
        self.assertNoInstallation(run)
        self.assertNothingWritten(run)

    def test_missing_release_binary_stops_before_any_installation(self):
        run = self.run_installer(("user",), env={"INSTALL_TEST_NO_BINARY": "1"})
        self.assertNotEqual(run.result.returncode, 0)
        self.assertIn("release binary missing", run.result.stderr)
        self.assertNoInstallation(run)
        self.assertNothingWritten(run)

    # -- user mode ----------------------------------------------------------

    def test_user_mode_installs_only_into_home(self):
        run = self.run_installer(("user",))
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        self.assertEqual(run.added("home"), with_parents(USER_FILES))
        self.assertEqual(run.added("tmpdir"), set())
        self.assertEqual(run.added("root"), {
            "target", "target/release", "target/release/on-air-desktop",
        })
        self.assertNoInstallation(run)
        desktop = (self.home / f".local/share/applications/{APP_ID}.desktop").read_text()
        self.assertIn(f"Exec={self.home}/.local/bin/on-air-desktop\n", desktop)

    def test_user_mode_rewrites_login_autostart(self):
        auto = self.home / ".config/autostart/on-air-desktop.desktop"
        auto.parent.mkdir(parents=True)
        auto.write_text("[Desktop Entry]\nExec=on-air-desktop --hidden\n")
        run = self.run_installer(("user",))
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        self.assertIn(f"Exec={self.home}/.local/bin/on-air-desktop --hidden\n", auto.read_text())

    # -- yay (pacman package) mode -----------------------------------------

    def test_yay_mode_stages_and_installs_package_with_sudo(self):
        run = self.run_installer(("yay",))
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        package = self.root / "packaging/dist/on-air-desktop-1-1-test.pkg.tar.zst"
        self.assertTrue(package.is_file(), run.result.stdout + run.result.stderr)
        self.assertEqual(
            run.called("sudo"),
            [["sudo", "-n", "true"], ["sudo", "pacman", "-U", "--noconfirm", str(package)]],
        )
        self.assertIn(["makepkg", "--noconfirm", "--nodeps"], run.calls)
        # The staging directory is cleaned up on success and on the way out.
        self.assertEqual(run.added("tmpdir"), set())
        self.assertEqual(run.added("home"), set())
        self.assertIn("Installed on-air-desktop", run.result.stdout)
        # Staging: the PKGBUILD packages exactly the staged files, at the
        # version pinned by tauri.conf.json.
        pkgbuild = (self.capture / "PKGBUILD").read_text()
        staged = set((self.capture / "staged.txt").read_text().splitlines())
        self.assertEqual(staged, STAGED_FILES)
        self.assertIn("pkgname=on-air-desktop", pkgbuild)
        self.assertIn(f"pkgver={VERSION}", pkgbuild)
        self.assertIn(f"/usr/bin/{'on-air-desktop'}", pkgbuild)
        self.assertIn("provides=('on-air-desktop')", pkgbuild)
        self.assertIn("conflicts=('on-air-desktop-bin')", pkgbuild)
        sources = pkgbuild_list(pkgbuild, "source")
        self.assertEqual(set(sources), STAGED_FILES - {"PKGBUILD"})
        self.assertEqual(len(pkgbuild_sums(pkgbuild)), 7)

    def test_yay_is_preferred_over_sudo_pacman(self):
        run = self.run_installer(("yay",), mocks=DEFAULT_MOCKS + ("yay",))
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        package = self.root / "packaging/dist/on-air-desktop-1-1-test.pkg.tar.zst"
        self.assertIn(["yay", "-U", "--noconfirm", str(package)], run.calls)
        self.assertEqual([c for c in run.called("sudo") if c[1:2] != ["-n"]], [])

    def test_yay_success_unshadows_stale_user_copy(self):
        binary = self.home / ".local/bin/on-air-desktop"
        binary.parent.mkdir(parents=True)
        binary.write_text("stale\n")
        desktop = self.home / f".local/share/applications/{APP_ID}.desktop"
        desktop.parent.mkdir(parents=True)
        desktop.write_text("stale\n")
        auto = self.home / ".config/autostart/on-air-desktop.desktop"
        auto.parent.mkdir(parents=True)
        auto.write_text("[Desktop Entry]\nExec=/home/x/.local/bin/on-air-desktop --hidden\n")
        run = self.run_installer(("yay",), env={"INSTALL_TEST_PACMAN_Q": "1"})
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        self.assertFalse(binary.exists())
        self.assertFalse(desktop.exists())
        self.assertIn("Exec=/usr/bin/on-air-desktop --hidden\n", auto.read_text())
        self.assertIn("Pointed login autostart at /usr/bin/on-air-desktop", run.result.stdout)

    # -- authentication failures preserve state -----------------------------

    def test_sudo_failure_keeps_package_and_stops(self):
        run = self.run_installer(("yay",), env={"INSTALL_TEST_NO_SUDO": "1"})
        self.assertEqual(run.result.returncode, 1, run.result.stdout)
        package = self.root / "packaging/dist/on-air-desktop-1-1-test.pkg.tar.zst"
        self.assertTrue(package.is_file(), "built package must survive for a manual install")
        self.assertEqual(list((self.root / "packaging/dist").iterdir()), [package])
        self.assertIn("The package is built", run.result.stderr)
        self.assertIn(f"sudo pacman -U {package}", run.result.stderr)
        # No installation was attempted and the staging directory is gone.
        self.assertEqual(run.called("pacman"), [])
        self.assertEqual(run.called("yay"), [])
        self.assertEqual(run.called("pkexec"), [])
        self.assertEqual([c for c in run.called("sudo") if c[1:2] != ["-n"]], [])
        self.assertEqual(run.added("tmpdir"), set())
        self.assertEqual(run.added("home"), set())

    def test_a_failing_package_install_is_reported_as_a_failure(self):
        # sudo works and the install command runs — it just fails. This is
        # different from test_sudo_failure_keeps_package_and_stops, where no
        # install is attempted at all. `install_as_root` used to `return 0`
        # unconditionally after the install command, so this case printed
        # "Installed on-air-desktop" and exited 0 on a failed install.
        run = self.run_installer(("yay",), env={"INSTALL_TEST_PACMAN_FAIL": "1"})
        self.assertEqual(run.result.returncode, 1, run.result.stdout)
        self.assertNotIn("Installed on-air-desktop", run.result.stdout)
        package = self.root / "packaging/dist/on-air-desktop-1-1-test.pkg.tar.zst"
        self.assertTrue(package.is_file(), "the built package must survive for a manual install")
        self.assertEqual(list((self.root / "packaging/dist").iterdir()), [package])
        self.assertIn("The package is built", run.result.stderr)
        self.assertIn(
            [c for c in run.called("sudo") if "pacman" in c],
            [[["sudo", "pacman", "-U", "--noconfirm", str(package)]]],
        )

    def test_pkexec_failure_keeps_package_and_stops(self):
        env = {"INSTALL_TEST_NO_SUDO": "1", "INSTALL_TEST_POLKIT_AGENT": "1",
               "INSTALL_TEST_PKEXEC_FAIL": "1"}
        run = self.run_installer(("yay",), env=env)
        self.assertEqual(run.result.returncode, 1, run.result.stdout)
        package = self.root / "packaging/dist/on-air-desktop-1-1-test.pkg.tar.zst"
        self.assertTrue(package.is_file())
        self.assertIn(["pkexec", "pacman", "-U", "--noconfirm", str(package)], run.calls)
        self.assertEqual(run.called("pacman"), [])
        self.assertEqual(run.added("tmpdir"), set())
        self.assertEqual(run.added("home"), set())

    def test_pkexec_installs_when_polkit_agent_is_running(self):
        env = {"INSTALL_TEST_NO_SUDO": "1", "INSTALL_TEST_POLKIT_AGENT": "1"}
        run = self.run_installer(("yay",), env=env)
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        package = self.root / "packaging/dist/on-air-desktop-1-1-test.pkg.tar.zst"
        self.assertIn(["pkexec", "pacman", "-U", "--noconfirm", str(package)], run.calls)

    # -- makepkg failure leaves only the staging directory ------------------

    def test_makepkg_failure_leaves_only_the_staging_directory(self):
        run = self.run_installer(("yay",), env={"INSTALL_TEST_MAKEPKG_FAIL": "1"})
        self.assertNotEqual(run.result.returncode, 0)
        # makepkg ran and failed; no installation was attempted after it.
        self.assertEqual(run.called("makepkg"), [["makepkg", "--noconfirm", "--nodeps"]])
        self.assertEqual(install_calls(run.calls), [], run.calls)
        self.assertEqual(run.called("sudo"), [])
        # Nothing outside the staging area: no package, no home writes; only
        # the build output and an empty dist directory.
        self.assertEqual(run.added("root"), {
            "packaging/dist", "target", "target/release",
            "target/release/on-air-desktop",
        })
        self.assertEqual(list((self.root / "packaging/dist").iterdir()), [])
        self.assertEqual(run.added("home"), set())
        # The staging directory is deliberately left behind (install.sh has
        # no cleanup trap): pinned so any change to that is deliberate.
        staged = run.added("tmpdir")
        self.assertEqual({p.split(os.sep)[0] for p in staged},
                         {p for p in staged if os.sep not in p})
        stage = self.tmpdir / next(iter(p for p in staged if os.sep not in p))
        self.assertTrue(stage.name.startswith("on-air-pkg."), staged)
        self.assertEqual({p.name for p in stage.iterdir()}, STAGED_FILES)
        # The staged PKGBUILD checksums match the staged files.
        pkgbuild = (stage / "PKGBUILD").read_text()
        sources = pkgbuild_list(pkgbuild, "source")
        sums = pkgbuild_sums(pkgbuild)
        self.assertEqual(len(sources), len(sums))
        for name, digest in zip(sources, sums):
            actual = hashlib.sha256((stage / name).read_bytes()).hexdigest()
            self.assertEqual(actual, digest, name)

    # -- flatpak mode -------------------------------------------------------

    def test_flatpak_mode_installs_with_flatpak_builder(self):
        run = self.run_installer(("flatpak",))
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        shared = self.root / "packaging/flatpak/shared-modules"
        generated = self.root / "packaging/flatpak/com.shay.on-air.local.yml"
        self.assertIn(
            ["git", "clone", "--depth", "1",
             "https://github.com/flathub/shared-modules.git", str(shared)],
            run.calls,
        )
        self.assertIn(
            ["flatpak", "remote-add", "--user", "--if-not-exists", "flathub",
             "https://dl.flathub.org/repo/flathub.flatpakrepo"],
            run.calls,
        )
        self.assertTrue(any("org.gnome.Platform//51" in c for c in run.called("flatpak")))
        self.assertIn(
            ["flatpak-builder", "--user", "--install", "--force-clean",
             "--disable-rofiles-fuse", str(self.root / "packaging/flatpak/build"),
             str(generated)],
            run.calls,
        )
        # The generated manifest points the source at this checkout over
        # file://; nothing in this suite reaches the network.
        text = generated.read_text()
        self.assertIn(f"url: file://{self.root}", text)
        self.assertIn("commit: " + "1" * 40, text)
        self.assertNotIn("\n        tag:", text)
        # No desktop build ran: the Flatpak fetches the commit itself.
        self.assertEqual(run.called("cargo"), [])
        self.assertEqual(run.called("pnpm"), [])
        self.assertEqual(run.called("makepkg"), [])
        self.assertEqual(run.added("home"), set())
        self.assertEqual(run.added("tmpdir"), set())
        self.assertEqual(run.added("root"), {
            "packaging/dist",
            "packaging/flatpak/shared-modules",
            "packaging/flatpak/shared-modules/libappindicator",
            "packaging/flatpak/shared-modules/libappindicator/libappindicator-gtk3-12.10.json",
            "packaging/flatpak/com.shay.on-air.local.yml",
        })

    def test_flatpak_mode_falls_back_to_org_flatpak_builder(self):
        mocks = tuple(m for m in DEFAULT_MOCKS if m != "flatpak-builder")
        run = self.run_installer(("flatpak",), mocks=mocks)
        self.assertEqual(run.result.returncode, 0, run.result.stderr)
        self.assertIn(
            ["flatpak", "run", "--user", f"--filesystem={self.root}",
             "org.flatpak.Builder", "--user", "--install", "--force-clean",
             "--disable-rofiles-fuse", str(self.root / "packaging/flatpak/build"),
             str(self.root / "packaging/flatpak/com.shay.on-air.local.yml")],
            run.calls,
        )
        self.assertEqual(run.called("flatpak-builder"), [])

    def test_flatpak_clone_failure_stops_before_installation(self):
        run = self.run_installer(("flatpak",), env={"INSTALL_TEST_GIT_FAIL": "1"})
        self.assertNotEqual(run.result.returncode, 0)
        self.assertNoInstallation(run)
        self.assertEqual(run.called("flatpak"), [])
        self.assertNothingWritten(run)


if __name__ == "__main__":
    unittest.main()
