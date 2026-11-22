# Homebrew cask template. Copy it to your tap as Casks/ai-usage-meter.rb:
#   github.com/<owner>/homebrew-tap
# Then users run: brew install --cask <owner>/tap/ai-usage-meter
# Fill in OWNER, the version and the sha256 from the release (dist/*.dmg.sha256).
cask "ai-usage-meter" do
  version "0.1.0"
  sha256 "REPLACE_WITH_SHA256_OF_THE_DMG"

  url "https://github.com/OWNER/ai-usage-meter/releases/download/v#{version}/AIUsageMeter-#{version}.dmg"
  name "AI Usage Meter"
  desc "Menu-bar app for AI coding tool usage, cost and plan limits"
  homepage "https://github.com/OWNER/ai-usage-meter"

  depends_on macos: ">= :sonoma"

  app "AIUsageMeter.app"

  zap trash: [
    "~/Library/Preferences/app.aiusagemeter.AIUsageMeter.plist",
    "~/Library/Caches/app.aiusagemeter.AIUsageMeter",
  ]
end
