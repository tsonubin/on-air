{
  lib,
  rustPlatform,
  pkg-config,
  alsa-lib,
  openssl,
  stdenv,
  src,
}:

rustPlatform.buildRustPackage rec {
  pname = "on-air-core";
  version = "0.1.0";

  inherit src;

  cargoLock.lockFile = ../../Cargo.lock;

  cargoBuildFlags = [
    "--package"
    "on-air-core"
    "--bin"
    "on-air-core"
  ];
  cargoTestFlags = [
    "--package"
    "on-air-core"
  ];

  nativeBuildInputs = [
    pkg-config
  ];

  buildInputs =
    lib.optionals stdenv.hostPlatform.isLinux [
      alsa-lib
      openssl
    ]
    ++ lib.optionals stdenv.hostPlatform.isDarwin [
      openssl
    ];

  # The default cargoInstallHook copies the built on-air-core binary to $out/bin.

  meta = {
    description = "LAN audio streaming core (HTTP/WS control + AirPlay/Bluetooth/Sonos senders)";
    homepage = "https://github.com/tsonubin/on-air";
    mainProgram = "on-air-core";
    platforms = lib.platforms.unix;
  };
}
