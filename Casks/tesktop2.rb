cask "tesktop2" do
  version "1.0.0-nightly.20260923.45"
  sha256 "7f5786a0e705f11e903120c5a8f543fa7a7a72d3cd1fad099b50cef11dd46015"

  url "https://github.com/ViceVerse-cz/Serein/releases/download/v#{version}/tesktop2-v#{version}-macOS-ARM64.zip"
  name "tesktop2"
  desc "Experimental native Discord client"
  homepage "https://github.com/ViceVerse-cz/Serein"

  depends_on arch: :arm64
  depends_on macos: :sonoma

  app "tesktop2.app"
end
