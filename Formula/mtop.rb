class Mtop < Formula
  desc "Cross-platform terminal system monitor"
  homepage "https://github.com/EvarinthoSec/mtop"
  version "0.1.0-alpha.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/EvarinthoSec/mtop/releases/download/v#{version}/mtop-macos-arm64-#{version}.tar.gz"
      sha256 "3229298bd712b18ea759a37a714d1b24786de6c2a3e950e277ad2a5332b0ec0b"
    end

    on_intel do
      url "https://github.com/EvarinthoSec/mtop/releases/download/v#{version}/mtop-macos-x86_64-#{version}.tar.gz"
      sha256 "9ffd8b1ea79f7d0606072c6621d29d9ee79829aeb0c3f4cdc2ad2fb16b3ac27c"
    end
  end

  def install
    bin.install "mtop"
  end

  test do
    assert_match "A fast terminal system monitor", shell_output("#{bin}/mtop --help")
  end
end
