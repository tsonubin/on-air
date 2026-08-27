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
    "--example"
    "serve"
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

  # Lib crate has no [[bin]]; cargo install would fail. Copy the example.
  installPhase = ''
    runHook preInstall
    found=$(find target -type f -path '*/release/examples/serve' | head -n1)
    if [ -z "$found" ]; then
      echo "on-air-core serve example was not built" >&2
      find target -name serve -o -name serve.exe >&2 || true
      exit 1
    fi
    install -Dm755 "$found" $out/bin/on-air-core
    runHook postInstall
  '';

  meta = {
    description = "LAN audio streaming core (HTTP/WS control + AirPlay/Bluetooth/Sonos senders)";
    homepage = "https://github.com/tsonubin/on-air";
    mainProgram = "on-air-core";
    platforms = lib.platforms.unix;
  };
}
