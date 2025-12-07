# Homebrew formula for Engraver
# To install: brew install mstephenholl/tap/engraver
# Or from local: brew install --build-from-source ./engraver.rb

class Engraver < Formula
  desc "Safe, fast tool for creating bootable USB drives"
  homepage "https://github.com/mstephenholl/engraver"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/mstephenholl/engraver/releases/download/v#{version}/engraver-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_ARM64"
    end
    on_intel do
      url "https://github.com/mstephenholl/engraver/releases/download/v#{version}/engraver-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X64"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/mstephenholl/engraver/releases/download/v#{version}/engraver-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_ARM64"
    end
    on_intel do
      url "https://github.com/mstephenholl/engraver/releases/download/v#{version}/engraver-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_LINUX_X64"
    end
  end

  def install
    bin.install "engraver"
    
    # Install shell completions
    bash_completion.install "completions/engraver.bash" => "engraver"
    zsh_completion.install "completions/_engraver"
    fish_completion.install "completions/engraver.fish"
    
    # Install man pages
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
    
    # Test checksum command
    (testpath/"test.txt").write("Hello, World!\n")
    output = shell_output("#{bin}/engraver checksum #{testpath}/test.txt")
    assert_match "SHA-256", output
    assert_match "c98c24b677eff44860afea6f493bbaec5bb1c4cbb209c6fc2bbb47f66ff2ad31", output
  end
end
