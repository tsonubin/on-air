# Binary spec: re-wrap the Tauri-produced GitHub Release RPM so COPR/local
# rpmbuild can consume a URL. Prefer installing that RPM directly:
#   sudo dnf install https://github.com/tsonubin/on-air/releases/download/v0.1.0/on-air-desktop-0.1.0-1.x86_64.rpm
#   sudo dnf install https://github.com/tsonubin/on-air/releases/download/v0.1.0/on-air-desktop-0.1.0-1.aarch64.rpm

Name:           on-air-desktop
Version:        0.1.0
Release:        1%{?dist}
Summary:        LAN audio streaming to AirPlay, Bluetooth, or Sonos (desktop GUI)

License:        Unspecified
URL:            https://github.com/tsonubin/on-air
%ifarch aarch64
Source0:        https://github.com/tsonubin/on-air/releases/download/v%{version}/on-air-desktop-%{version}-1.aarch64.rpm
%else
Source0:        https://github.com/tsonubin/on-air/releases/download/v%{version}/on-air-desktop-%{version}-1.x86_64.rpm
%endif

ExclusiveArch:  x86_64 aarch64

# Tauri 2 Linux GUI
Requires:       webkit2gtk4.1
Requires:       gtk3
Requires:       libappindicator-gtk3
Requires:       librsvg2
Requires:       alsa-lib

%description
Desktop tray app for on-air. Embeds the core control plane (port 47990)
and a WebView UI.

%prep
true

%build
true

%install
mkdir -p %{buildroot}
rpm2cpio %{SOURCE0} | cpio -idmv -D %{buildroot}

%files
%defattr(-,root,root,-)
%{_bindir}/on-air-desktop
%{_datadir}/applications
%{_datadir}/icons

%changelog
* Thu Aug 27 2026 Shay <tsonubin@users.noreply.github.com> - 0.1.0-1
- Initial binary wrap of the Tauri GitHub Release RPM
