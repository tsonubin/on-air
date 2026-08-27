#!/usr/bin/env bash
# Fill SHA-256 pins in Homebrew / AUR / winget from a published GitHub Release.
set -euo pipefail

VERSION="${1:-0.1.0}"
REPO="${REPO:-tsonubin/on-air}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if ! command -v gh >/dev/null; then
  echo "gh is required" >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if ! gh release download "v${VERSION}" --repo "$REPO" --dir "$tmp" --pattern 'SHA256SUMS' 2>/dev/null; then
  echo "No SHA256SUMS on v${VERSION}; hashing assets directly..."
  gh release download "v${VERSION}" --repo "$REPO" --dir "$tmp" || {
    echo "Could not download v${VERSION} from ${REPO}. Publish a Release first." >&2
    exit 1
  }
  (cd "$tmp" && shasum -a 256 * > SHA256SUMS)
fi

hash_of() {
  local name="$1"
  awk -v f="$name" '$2 == f { print $1; found=1 } END { if (!found) exit 1 }' "$tmp/SHA256SUMS"
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
deb=$(try_hash "on-air-desktop_${VERSION}_amd64.deb" || true)
nsis=$(try_hash "on-air-desktop_${VERSION}_x64-setup.exe" || true)
msi=$(try_hash "on-air-desktop_${VERSION}_x64_en-US.msi" || true)

if [ -n "${dmg_arm:-}" ] && [ -n "${dmg_intel:-}" ]; then
  python3 - "$root/Casks/on-air.rb" "$dmg_arm" "$dmg_intel" <<'PY'
import pathlib, re, sys
path, arm, intel = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
text = path.read_text()
text = re.sub(
    r"sha256 :no_check",
    f'sha256 arm: "{arm}",\n         intel: "{intel}"',
    text,
    count=1,
)
# Subsequent runs already have sha256 arm:/intel:
text = re.sub(r'(sha256 arm: ")[0-9a-f]+(")', rf"\g<1>{arm}\2", text, count=1)
text = re.sub(r'(intel: ")[0-9a-f]+(")', rf"\g<1>{intel}\2", text, count=1)
path.write_text(text)
print(f"updated {path}")
PY
else
  echo "skipping Casks/on-air.rb (need both macOS dmgs)" >&2
fi

if [ -n "${deb:-}" ]; then
  python3 - "$root/packaging/aur/on-air-bin/PKGBUILD" "$deb" <<'PY'
import pathlib, re, sys
path, digest = pathlib.Path(sys.argv[1]), sys.argv[2]
text = path.read_text()
text = re.sub(r"sha256sums=\('SKIP'\)", f"sha256sums=('{digest}')", text, count=1)
text = re.sub(r"sha256sums=\('[0-9a-f]+'\)", f"sha256sums=('{digest}')", text, count=1)
path.write_text(text)
print(f"updated {path}")
PY
  if command -v makepkg >/dev/null; then
    (cd "$root/packaging/aur/on-air-bin" && makepkg --printsrcinfo > .SRCINFO)
  else
    python3 - "$root/packaging/aur/on-air-bin/.SRCINFO" "$deb" <<'PY'
import pathlib, re, sys
path, digest = pathlib.Path(sys.argv[1]), sys.argv[2]
text = path.read_text()
text = re.sub(r"sha256sums = \S+", f"sha256sums = {digest}", text, count=1)
path.write_text(text)
print(f"updated {path}")
PY
  fi
else
  echo "skipping AUR on-air-bin (need .deb)" >&2
fi

if [ -n "${nsis:-}" ] || [ -n "${msi:-}" ]; then
  python3 - "$root/packaging/winget/Tsonubin.OnAir.installer.yaml" "${nsis:-}" "${msi:-}" <<'PY'
import pathlib, re, sys
path = pathlib.Path(sys.argv[1])
nsis, msi = sys.argv[2], sys.argv[3]
text = path.read_text()
hashes = []
if nsis:
    hashes.append(nsis.upper())
if msi:
    hashes.append(msi.upper())
it = iter(hashes)

def sub(_match):
    try:
        return f"InstallerSha256: {next(it)}"
    except StopIteration:
        return _match.group(0)

path.write_text(re.sub(r"InstallerSha256: [0-9A-Fa-f]{64}", sub, text))
print(f"updated {path}")
PY
else
  echo "skipping winget (need NSIS and/or MSI)" >&2
fi

echo "Done. Commit the updated hashes with the packaging files."
