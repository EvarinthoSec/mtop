class Mtop < Formula
  desc "Cross-platform terminal system monitor"
  homepage "https://github.com/EvarinthoSec/mtop"
  version "0.1.0-alpha.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/EvarinthoSec/mtop/releases/download/v#{version}/mtop-macos-arm64-#{version}.tar.gz"
      sha256 "afc94d90ead250646bc5908c6813380022031518340aeefb9d2ef1be6752fa81"
    end

    on_intel do
      url "https://github.com/EvarinthoSec/mtop/releases/download/v#{version}/mtop-macos-x86_64-#{version}.tar.gz"
      sha256 "87add613ae3af40ebf2809385edfa3e855112e06cd2f92d7726f4dcda7de9497"
    end
  end

  def install
    bin.install "mtop"
  end

  test do
    assert_match "A fast terminal system monitor", shell_output("#{bin}/mtop --help")
  end
end
