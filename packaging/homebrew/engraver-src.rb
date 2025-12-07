# Homebrew formula for Engraver (build from source)
# To install: brew install --build-from-source ./engraver-src.rb

class EngraverSrc < Formula
  desc "Safe, fast tool for creating bootable USB drives"
  homepage "https://github.com/mstephenholl/engraver"
  url "https://github.com/mstephenholl/engraver/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER_SHA256_SOURCE"
  license "MIT"
  head "https://github.com/mstephenholl/engraver.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "-p", "engraver"
    bin.install "target/release/engraver"
    
    # Generate and install shell completions
    output = Utils.safe_popen_read(bin/"engraver", "completions", "bash")
    (bash_completion/"engraver").write output
    output = Utils.safe_popen_read(bin/"engraver", "completions", "zsh")
    (zsh_completion/"_engraver").write output
    output = Utils.safe_popen_read(bin/"engraver", "completions", "fish")
    (fish_completion/"engraver.fish").write output
    
    # Generate and install man pages
    mkdir "man"
    system bin/"engraver", "mangen", "--out-dir", "man"
    man1.install Dir["man/*.1"]
  end

  def caveats
    <<~EOS
      Engraver requires root privileges to write to devices.
      Use: sudo engraver write <image> <device>

      To get started:
        engraver list           # List available drives
        engraver --help         # Show all commands
    EOS
  end

  test do
    assert_match "engraver #{version}", shell_output("#{bin}/engraver --version")
    assert_match "List available drives", shell_output("#{bin}/engraver --help")
  end
end
