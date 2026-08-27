Name:           on-air-core
Version:        0.1.0
Release:        1%{?dist}
Summary:        LAN audio streaming core (AirPlay, Bluetooth, Sonos)

License:        Unspecified
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
and Sonos.

%prep
%autosetup -n on-air-%{version}

%build
cargo build --release --locked --package on-air-core --example serve

%install
install -D -m 0755 target/release/examples/serve %{buildroot}%{_bindir}/on-air-core

%check
cargo test --release --locked --package on-air-core

%files
%{_bindir}/on-air-core

%changelog
* Thu Aug 27 2026 Shay <tsonubin@users.noreply.github.com> - 0.1.0-1
- Initial package
