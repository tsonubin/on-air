# on-air

LAN-only desktop audio streaming to **one** of AirPlay, Bluetooth, or Sonos,
with a tray app, login autostart, and a mobile remote on the same network.

**Install:** see [INSTALL.md](INSTALL.md) (Homebrew, winget, APT, AUR/yay, RPM, Nix, or source).

Control plane listens on port `47990`. Pair from the LAN with a PIN. Transports
are exclusive — switching outputs tears down the current sender first.
