Name:           on-air-core
Version:        0.2.0
Release:        1%{?dist}
Summary:        LAN audio streaming core (AirPlay, Bluetooth, Sonos)

License:        MIT
URL:            https://github.com/tsonubin/on-air
Source0:        https://github.com/tsonubin/on-air/archive/refs/tags/v%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  pkgconf
BuildRequires:  alsa-lib-devel
BuildRequires:  openssl-devel

%description
Headless on-air control plane: HTTP/WebSocket on port 47990, mDNS
advertisement, PIN pairing, and exclusive senders for AirPlay, Bluetooth,
and Sonos. Start it with `systemctl --user enable --now on-air-core`.

%prep
%autosetup -n on-air-%{version}

%build
cargo build --release --locked --package on-air-core --bin on-air-core

%install
install -D -m 0755 target/release/on-air-core %{buildroot}%{_bindir}/on-air-core
install -D -m 0644 packaging/omarchy/on-air-core.service %{buildroot}%{_userunitdir}/on-air-core.service

%check
cargo test --release --locked --package on-air-core

%files
%{_bindir}/on-air-core
%{_userunitdir}/on-air-core.service

%changelog
* Tue Oct 13 2026 Shay <tsonubin@users.noreply.github.com> - 0.2.0-1
- Persist settings and paired phones; clean shutdown on SIGTERM; MIT license

* Thu Aug 27 2026 Shay <tsonubin@users.noreply.github.com> - 0.1.0-1
- Initial package
