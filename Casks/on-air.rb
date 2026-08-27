cask "on-air" do
  arch arm: "aarch64", intel: "x64"

  version "0.1.0"
  sha256 arm:   "7867443e1dbc7ed2ce40b80e248e860a19305dd103bbe798dc593ac60d1b7aff",
         intel: "243e8906b3d006687f23980d0cd53b3f1b4f7c22a66e74e82b6fb66108a6e479"

  url "https://github.com/tsonubin/on-air/releases/download/v#{version}/on-air-desktop_#{version}_#{arch}.dmg"
  name "on-air"
  desc "LAN audio streaming to AirPlay, Bluetooth, or Sonos"
  homepage "https://github.com/tsonubin/on-air"

  livecheck do
    url :homepage
    strategy :github_latest
  end

  depends_on macos: :monterey

  app "on-air-desktop.app"

  zap trash: [
    "~/Library/Application Support/com.shay.on-air",
    "~/Library/Caches/com.shay.on-air",
    "~/Library/Logs/com.shay.on-air",
    "~/Library/Preferences/com.shay.on-air.plist",
    "~/Library/WebKit/com.shay.on-air",
  ]
end
