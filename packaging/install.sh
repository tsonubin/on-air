#!/usr/bin/env bash
# Install the on-air desktop app from this checkout.
#
#   packaging/install.sh            Arch + yay: pacman package. Otherwise Flatpak.
#   packaging/install.sh yay        build this tree and install it with yay
#   packaging/install.sh flatpak    per-user Flatpak (com.shay.on-air)
#   packaging/install.sh user       ~/.local, no package manager
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="com.shay.on-air"
DIST="${ROOT}/packaging/dist"

log() { printf '==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

setup_toolchain() {
  if [[ -x "${HOME}/.cargo/bin/cargo" ]]; then
    export PATH="${HOME}/.cargo/bin:${PATH}"
  fi
  if ! command -v pnpm >/dev/null 2>&1; then
    command -v npm >/dev/null 2>&1 || die "pnpm or npm is required"
    local prefix="${HOME}/.local/share/on-air-pnpm"
    npm install --prefix "$prefix" pnpm@9.15.0
    export PATH="${prefix}/node_modules/.bin:${PATH}"
  fi
  command -v cargo >/dev/null 2>&1 || die "cargo is required"
  command -v pnpm >/dev/null 2>&1 || die "pnpm is required"
  if ! pkg-config --exists webkit2gtk-4.1 gtk+-3.0 2>/dev/null; then
    die "webkit2gtk-4.1 and gtk3 development files are required"
  fi
}

build_release() {
  setup_toolchain
  log "Building on-air-desktop from ${ROOT}"
  cd "$ROOT"
  pnpm install --frozen-lockfile --filter 'desktop...'
  pnpm --filter desktop build
  cargo build --locked --release -p on-air-desktop
  [[ -x "${ROOT}/target/release/on-air-desktop" ]] || die "release binary missing"
}

sha256_file() { sha256sum "$1" | awk '{print $1}'; }

# Package the binary just built and install it with yay (or makepkg -si).
# The recipe is written here because it packages a local build, not a
# release; on-air-desktop-bin under packaging/aur is the published shape.
install_yay() {
  command -v makepkg >/dev/null 2>&1 || die "makepkg is required (base-devel)"
  build_release

  local stage version arch_name
  version="$(sed -n 's/^  "version": "\([^"]*\)".*/\1/p' "${ROOT}/apps/desktop/src-tauri/tauri.conf.json" | head -1)"
  [[ -n "$version" ]] || die "could not read the app version"
  arch_name="$(uname -m)"
  stage="$(mktemp -d "${TMPDIR:-/tmp}/on-air-pkg.XXXXXX")"
  mkdir -p "$DIST"

  install -Dm755 "${ROOT}/target/release/on-air-desktop" "${stage}/on-air-desktop"
  install -Dm644 "${ROOT}/LICENSE" "${stage}/LICENSE"
  install -Dm644 "${ROOT}/packaging/flatpak/com.shay.on-air.desktop" \
    "${stage}/com.shay.on-air.desktop"
  install -Dm644 "${ROOT}/apps/desktop/src-tauri/icons/32x32.png" "${stage}/icon-32.png"
  install -Dm644 "${ROOT}/apps/desktop/src-tauri/icons/128x128.png" "${stage}/icon-128.png"
  install -Dm644 "${ROOT}/apps/desktop/src-tauri/icons/128x128@2x.png" "${stage}/icon-256.png"
  install -Dm644 "${ROOT}/apps/desktop/src-tauri/icons/icon.png" "${stage}/icon-512.png"

  cat > "${stage}/PKGBUILD" <<EOF
pkgname=on-air-desktop
pkgver=${version}
pkgrel=1
pkgdesc='LAN audio streaming to AirPlay, Bluetooth, or Sonos (desktop GUI)'
arch=('${arch_name}')
url='https://github.com/tsonubin/on-air'
license=('MIT')
depends=(
  'gtk3'
  'webkit2gtk-4.1'
  'libayatana-appindicator'
  'librsvg'
  'alsa-lib'
  'openssl'
  'hicolor-icon-theme'
  'libpulse'
  'bluez-utils'
  'python'
)
provides=('on-air-desktop')
conflicts=('on-air-desktop-bin')
options=('!strip' '!debug')
source=(
  on-air-desktop
  LICENSE
  com.shay.on-air.desktop
  icon-32.png
  icon-128.png
  icon-256.png
  icon-512.png
)
sha256sums=(
  '$(sha256_file "${stage}/on-air-desktop")'
  '$(sha256_file "${stage}/LICENSE")'
  '$(sha256_file "${stage}/com.shay.on-air.desktop")'
  '$(sha256_file "${stage}/icon-32.png")'
  '$(sha256_file "${stage}/icon-128.png")'
  '$(sha256_file "${stage}/icon-256.png")'
  '$(sha256_file "${stage}/icon-512.png")'
)

package() {
  install -Dm755 on-air-desktop "\${pkgdir}/usr/bin/on-air-desktop"
  install -Dm644 LICENSE "\${pkgdir}/usr/share/licenses/on-air-desktop/LICENSE"
  install -Dm644 com.shay.on-air.desktop "\${pkgdir}/usr/share/applications/${APP_ID}.desktop"
  install -Dm644 icon-32.png "\${pkgdir}/usr/share/icons/hicolor/32x32/apps/${APP_ID}.png"
  install -Dm644 icon-128.png "\${pkgdir}/usr/share/icons/hicolor/128x128/apps/${APP_ID}.png"
  install -Dm644 icon-256.png "\${pkgdir}/usr/share/icons/hicolor/256x256/apps/${APP_ID}.png"
  install -Dm644 icon-512.png "\${pkgdir}/usr/share/icons/hicolor/512x512/apps/${APP_ID}.png"
  install -Dm644 icon-256.png "\${pkgdir}/usr/share/icons/hicolor/256x256/apps/on-air-desktop.png"
}
EOF

  log "Packing ${stage}"
  (
    cd "$stage"
    makepkg --noconfirm --nodeps
  )
  local pkgfile
  pkgfile="$(find "$stage" -maxdepth 1 -name 'on-air-desktop-*.pkg.tar.*' -print -quit)"
  [[ -n "$pkgfile" ]] || die "makepkg did not produce a package"
  cp -f "$pkgfile" "$DIST/"
  pkgfile="${DIST}/$(basename "$pkgfile")"
  rm -rf "$stage"
  log "Package: ${pkgfile}"

  if ! install_as_root "$pkgfile"; then
    local install_cmd="sudo pacman -U"
    if command -v yay >/dev/null 2>&1; then
      install_cmd="yay -U --noconfirm"
    fi
    cat <<EOF >&2

The package is built. Installing it needs your password:

  ${install_cmd} ${pkgfile}

EOF
    exit 1
  fi
  unshadow_user_copy
  log "Installed on-air-desktop. Launch it from the app menu, or run: on-air-desktop"
}

install_as_root() {
  local pkgfile="$1"
  if sudo -n true 2>/dev/null; then
    if command -v yay >/dev/null 2>&1; then
      yay -U --noconfirm "$pkgfile"
    else
      sudo pacman -U --noconfirm "$pkgfile"
    fi
    return 0
  fi
  # pkexec only works when a graphical polkit agent is running. Without one it waits forever.
  if command -v pkexec >/dev/null 2>&1 && pgrep -f 'polkit-gnome-authentication-agent|hyprpolkitagent|xfce-polkit|mate-polkit|lxqt-policykit-agent|polkit-kde-authentication' >/dev/null 2>&1; then
    log "Asking polkit to install ${pkgfile}"
    if pkexec pacman -U --noconfirm "$pkgfile"; then
      return 0
    fi
  fi
  return 1
}

# Drop the old ~/.local copy so the pacman package is the one that launches.
unshadow_user_copy() {
  pacman -Q on-air-desktop >/dev/null 2>&1 || return 0
  if [[ -e "${HOME}/.local/bin/on-air-desktop" ]]; then
    rm -f "${HOME}/.local/bin/on-air-desktop"
    log "Removed ${HOME}/.local/bin/on-air-desktop so /usr/bin/on-air-desktop is used"
  fi
  if [[ -f "${HOME}/.local/share/applications/${APP_ID}.desktop" ]]; then
    rm -f "${HOME}/.local/share/applications/${APP_ID}.desktop"
  fi
  local auto="${HOME}/.config/autostart/on-air-desktop.desktop"
  if [[ -f "$auto" ]]; then
    sed -i 's|^Exec=.*|Exec=/usr/bin/on-air-desktop --hidden|' "$auto"
    log "Pointed login autostart at /usr/bin/on-air-desktop"
  fi
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${HOME}/.local/share/applications" >/dev/null 2>&1 || true
  fi
}

install_user() {
  build_release
  local dest="${HOME}/.local"
  log "Installing into ${dest}"
  install -Dm755 "${ROOT}/target/release/on-air-desktop" "${dest}/bin/on-air-desktop"
  install -Dm644 "${ROOT}/packaging/flatpak/com.shay.on-air.desktop" \
    "${dest}/share/applications/${APP_ID}.desktop"
  sed -i "s|^Exec=.*|Exec=${dest}/bin/on-air-desktop|" \
    "${dest}/share/applications/${APP_ID}.desktop"
  install -Dm644 "${ROOT}/apps/desktop/src-tauri/icons/32x32.png" \
    "${dest}/share/icons/hicolor/32x32/apps/${APP_ID}.png"
  install -Dm644 "${ROOT}/apps/desktop/src-tauri/icons/128x128.png" \
    "${dest}/share/icons/hicolor/128x128/apps/${APP_ID}.png"
  install -Dm644 "${ROOT}/apps/desktop/src-tauri/icons/128x128@2x.png" \
    "${dest}/share/icons/hicolor/256x256/apps/${APP_ID}.png"
  install -Dm644 "${ROOT}/apps/desktop/src-tauri/icons/128x128@2x.png" \
    "${dest}/share/icons/hicolor/256x256/apps/on-air-desktop.png"
  install -Dm644 "${ROOT}/apps/desktop/src-tauri/icons/icon.png" \
    "${dest}/share/icons/hicolor/512x512/apps/${APP_ID}.png"
  local auto="${HOME}/.config/autostart/on-air-desktop.desktop"
  if [[ -f "$auto" ]]; then
    sed -i "s|^Exec=.*|Exec=${dest}/bin/on-air-desktop --hidden|" "$auto"
  fi
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${dest}/share/applications" >/dev/null 2>&1 || true
  fi
  log "Installed ${dest}/bin/on-air-desktop"
}

install_flatpak() {
  command -v flatpak >/dev/null 2>&1 || die "flatpak is required"
  local manifest="${ROOT}/packaging/flatpak/com.shay.on-air.yml"
  local shared="${ROOT}/packaging/flatpak/shared-modules"
  local commit
  commit="$(git -C "$ROOT" rev-parse HEAD)"

  if [[ -n "$(git -C "$ROOT" status --porcelain --untracked-files=no)" ]]; then
    log "Working tree has uncommitted changes. Flatpak builds commit ${commit}."
  fi

  if [[ ! -f "${shared}/libappindicator/libappindicator-gtk3-12.10.json" ]]; then
    log "Cloning Flathub shared-modules for the tray library"
    git clone --depth 1 https://github.com/flathub/shared-modules.git "$shared"
  fi
  flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
  log "Installing the GNOME 51 SDK and Flatpak builder (user install)"
  flatpak install --user -y --noninteractive flathub \
    org.gnome.Platform//51 \
    org.gnome.Sdk//51 \
    org.freedesktop.Sdk.Extension.rust-stable//26.08 \
    org.freedesktop.Sdk.Extension.node24//26.08 \
    org.flatpak.Builder

  # File sources and shared-modules are resolved next to the manifest.
  local generated="${ROOT}/packaging/flatpak/com.shay.on-air.local.yml"
  mkdir -p "$DIST"
  sed -E "s|^        url: .*|        url: file://${ROOT}|" "$manifest" \
    | sed -E "s#^        (tag|commit): .*#        commit: ${commit}#" > "$generated"

  local builder=(flatpak-builder --user --install --force-clean --disable-rofiles-fuse)
  local build_dir="${ROOT}/packaging/flatpak/build"
  if command -v flatpak-builder >/dev/null 2>&1; then
    log "Building Flatpak with flatpak-builder"
    "${builder[@]}" "$build_dir" "$generated"
  else
    log "Building Flatpak with org.flatpak.Builder"
    # The default command is flatpak-builder-wrapper, which points the
    # builder at this user's Flatpak installation. Overriding the command
    # skips that and cannot see the SDK.
    flatpak run --user --filesystem="$ROOT" org.flatpak.Builder \
      --user --install --force-clean --disable-rofiles-fuse \
      "$build_dir" "$generated"
  fi
  log "Installed Flatpak ${APP_ID}. Launch it with: flatpak run ${APP_ID}"
}

method="${1:-auto}"
case "$method" in
  auto)
    if [[ -f /etc/arch-release ]] && command -v makepkg >/dev/null 2>&1; then
      method=yay
    elif command -v flatpak >/dev/null 2>&1; then
      method=flatpak
    else
      method=user
    fi
    ;;
esac

case "$method" in
  yay | arch | pacman) install_yay ;;
  flatpak) install_flatpak ;;
  user | local) install_user ;;
  -h | --help | help)
    sed -n '2,8p' "$0"
    ;;
  *)
    die "unknown method '${method}'. Use yay, flatpak, or user."
    ;;
esac
