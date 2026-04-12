cask "synrepo" do
  arch arm: "arm64", intel: "x86_64"

  version "__VERSION__"
  sha256 arm:   "__ARM_SHA256__",
         intel: "__X86_SHA256__"

  url "https://github.com/whit3rabbit/synrepo/releases/download/v#{version}/synrepo-#{version}-macos-#{arch}.zip"
  name "synrepo"
  desc "A context compiler for AI coding agents"
  homepage "https://github.com/whit3rabbit/synrepo"

  binary "synrepo"
end
