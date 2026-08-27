class OnAirCore < Formula
  desc "LAN audio streaming core (AirPlay, Bluetooth, Sonos)"
  homepage "https://github.com/tsonubin/on-air"
  url "https://github.com/tsonubin/on-air.git",
      tag:      "v0.1.0",
      revision: "5e372829c846cdadacac1ebe8f4c1f5a4acec0b7"
  license :cannot_represent
  head "https://github.com/tsonubin/on-air.git", branch: "main"

  depends_on "pkg-config" => :build
  depends_on "rust" => :build
  on_linux do
    depends_on "alsa-lib"
    depends_on "openssl@3"
  end

  def install
    system "cargo", "install", *std_cargo_args(path: "packages/core"), "--example", "serve"
    mv bin/"serve", bin/"on-air-core"
  end

  service do
    run [opt_bin/"on-air-core"]
    keep_alive true
    working_dir var
    log_path var/"log/on-air-core.log"
    error_log_path var/"log/on-air-core.log"
    environment_variables PORT: "47990"
  end

  test do
    assert_path_exists bin/"on-air-core"
  end
end
