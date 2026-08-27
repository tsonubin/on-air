cask "on-air" do
  arch arm: "aarch64", intel: "x64"

  version "0.1.0"
  # sha256 filled by packaging/update-checksums.sh after a GitHub Release.
  sha256 :no_check

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
