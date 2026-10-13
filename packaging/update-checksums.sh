#!/usr/bin/env bash
# Fill SHA-256 pins in Homebrew / AUR / winget from a published GitHub
# Release, and pin the Homebrew formula to the tag's commit.
#   packaging/update-checksums.sh 0.2.0
#   SHA256SUMS_FILE=./SHA256SUMS packaging/update-checksums.sh 0.2.0
set -euo pipefail

VERSION="${1:-0.2.0}"
REPO="${REPO:-tsonubin/on-air}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# SHA256SUMS from, in order: $SHA256SUMS_FILE, gh, or a plain download
# (the repository is public, so no credentials are needed).
if [ -n "${SHA256SUMS_FILE:-}" ]; then
  cp "$SHA256SUMS_FILE" "$tmp/SHA256SUMS"
elif command -v gh >/dev/null && gh auth status >/dev/null 2>&1 &&
  gh release download "v${VERSION}" --repo "$REPO" --dir "$tmp" --pattern 'SHA256SUMS' 2>/dev/null; then
  :
elif curl -fsSL -o "$tmp/SHA256SUMS" \
  "https://github.com/${REPO}/releases/download/v${VERSION}/SHA256SUMS"; then
  :
else
  echo "Could not get SHA256SUMS for v${VERSION} from ${REPO}. Publish the Release first." >&2
  exit 1
fi

hash_of() {
  local name="$1"
  awk -v f="$name" '$2 == f || $2 == "*" f { print $1; found=1 } END { if (!found) exit 1 }' "$tmp/SHA256SUMS"
}

echo "=== SHA256SUMS (v${VERSION}) ==="
cat "$tmp/SHA256SUMS"
echo

try_hash() {
  local name="$1"
  if hash_of "$name"; then
    return 0
  fi
  echo "missing asset: $name" >&2
  return 1
}

dmg_arm=$(try_hash "on-air-desktop_${VERSION}_aarch64.dmg" || true)
dmg_intel=$(try_hash "on-air-desktop_${VERSION}_x64.dmg" || true)
deb_amd64=$(try_hash "on-air-desktop_${VERSION}_amd64.deb" || true)
deb_arm64=$(try_hash "on-air-desktop_${VERSION}_arm64.deb" || true)
nsis_x64=$(try_hash "on-air-desktop_${VERSION}_x64-setup.exe" || true)
msi_x64=$(try_hash "on-air-desktop_${VERSION}_x64_en-US.msi" || true)
nsis_arm64=$(try_hash "on-air-desktop_${VERSION}_arm64-setup.exe" || true)
msi_arm64=$(try_hash "on-air-desktop_${VERSION}_arm64_en-US.msi" || true)
core_x86=$(try_hash "on-air-core-serve-linux-x86_64" || true)
core_arm=$(try_hash "on-air-core-serve-linux-aarch64" || true)
tui_x86=$(try_hash "on-air-tui-linux-x86_64" || true)
tui_arm=$(try_hash "on-air-tui-linux-aarch64" || true)

# Homebrew cask: the two macOS disk images.
if [ -n "${dmg_arm:-}" ] && [ -n "${dmg_intel:-}" ]; then
  python3 - "$root/Casks/on-air.rb" "$dmg_arm" "$dmg_intel" <<'PY'
import pathlib, re, sys
path, arm, intel = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
text = path.read_text()
text = re.sub(r"  # Filled by packaging/update-checksums\.sh[^\n]*\n", "", text)
text = re.sub(
    r"sha256 :no_check",
    f'sha256 arm:   "{arm}",\n         intel: "{intel}"',
    text,
    count=1,
)
# Later runs already have sha256 arm:/intel:
text = re.sub(r'(sha256 arm:\s+")[0-9a-f]+(")', rf"\g<1>{arm}\2", text, count=1)
text = re.sub(r'(intel: ")[0-9a-f]+(")', rf"\g<1>{intel}\2", text, count=1)
path.write_text(text)
print(f"updated {path}")
PY
else
  echo "skipping Casks/on-air.rb (need both macOS dmgs)" >&2
fi

# Homebrew formula: pin the commit the tag points at.
rev=$(git ls-remote "https://github.com/${REPO}.git" "refs/tags/v${VERSION}^{}" "refs/tags/v${VERSION}" 2>/dev/null |
  awk '/\^\{\}$/ { peeled = $1 } !/\^\{\}$/ { plain = $1 } END { print (peeled != "" ? peeled : plain) }')
if [ -n "$rev" ]; then
  python3 - "$root/Formula/on-air-core.rb" "$VERSION" "$rev" <<'PY'
import pathlib, re, sys
path, version, rev = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
text = path.read_text()
text = re.sub(
    r'tag:\s+"v[^"]+"(,\s*\n\s*revision:\s+"[0-9a-f]+")?',
    f'tag:      "v{version}",\n      revision: "{rev}"',
    text,
    count=1,
)
path.write_text(text)
print(f"updated {path}")
PY
else
  echo "skipping Formula/on-air-core.rb (tag v${VERSION} not found)" >&2
fi

# AUR on-air-bin: the prebuilt core and terminal UI, per architecture.
if [ -n "${core_x86:-}" ] || [ -n "${core_arm:-}" ]; then
  python3 - "$root/packaging/aur/on-air-bin/PKGBUILD" \
    "${core_x86:-}" "${tui_x86:-}" "${core_arm:-}" "${tui_arm:-}" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
core_x86, tui_x86, core_arm, tui_arm = sys.argv[2:6]
text = path.read_text()
for arch, core, tui in (("x86_64", core_x86, tui_x86), ("aarch64", core_arm, tui_arm)):
    if core and tui:
        text = re.sub(
            rf"sha256sums_{arch}=\([^)]*\)",
            f"sha256sums_{arch}=('{core}' '{tui}')",
            text,
            count=1,
        )
path.write_text(text)
print(f"updated {path}")
PY
  bash "$root/packaging/aur/srcinfo.sh" "$root/packaging/aur/on-air-bin" > "$root/packaging/aur/on-air-bin/.SRCINFO"
else
  echo "skipping AUR on-air-bin (need the core and tui binaries)" >&2
fi

# AUR on-air-desktop-bin: the WebKitGTK desktop .deb.
if [ -n "${deb_amd64:-}" ] || [ -n "${deb_arm64:-}" ]; then
  python3 - "$root/packaging/aur/on-air-desktop-bin/PKGBUILD" "${deb_amd64:-}" "${deb_arm64:-}" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
amd64, arm64 = sys.argv[2], sys.argv[3]
text = path.read_text()
if amd64:
    text = re.sub(r"sha256sums_x86_64=\('(?:SKIP|[0-9a-f]+)'\)", f"sha256sums_x86_64=('{amd64}')", text, count=1)
if arm64:
    text = re.sub(r"sha256sums_aarch64=\('(?:SKIP|[0-9a-f]+)'\)", f"sha256sums_aarch64=('{arm64}')", text, count=1)
path.write_text(text)
print(f"updated {path}")
PY
  bash "$root/packaging/aur/srcinfo.sh" "$root/packaging/aur/on-air-desktop-bin" > "$root/packaging/aur/on-air-desktop-bin/.SRCINFO"
else
  echo "skipping AUR on-air-desktop-bin (need .deb)" >&2
fi

# winget: NSIS and MSI installers.
if [ -n "${nsis_x64:-}" ] || [ -n "${msi_x64:-}" ] || [ -n "${nsis_arm64:-}" ] || [ -n "${msi_arm64:-}" ]; then
  python3 - "$root/packaging/winget/Tsonubin.OnAir.installer.yaml" \
    "${nsis_x64:-}" "${msi_x64:-}" "${nsis_arm64:-}" "${msi_arm64:-}" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
repl = [h.upper() for h in sys.argv[2:] if h]
it = iter(repl)

def sub(_match):
    try:
        return f"InstallerSha256: {next(it)}"
    except StopIteration:
        return _match.group(0)

path.write_text(re.sub(r"InstallerSha256: [0-9A-Fa-f]{64}", sub, path.read_text()))
print(f"updated {path}")
PY
else
  echo "skipping winget (need NSIS and/or MSI)" >&2
fi

echo "Done. Commit the updated hashes with the packaging files."
