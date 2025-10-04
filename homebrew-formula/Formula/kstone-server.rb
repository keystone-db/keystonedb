# KeystoneDB Server Homebrew Formula
#
# Installation:
#   brew tap keystone-db/keystonedb
#   brew install kstone-server
#
# Note: Update SHA256 checksums after each release

class KstoneServer < Formula
  desc "KeystoneDB gRPC server for remote database access"
  homepage "https://github.com/keystone-db/keystonedb"
  version "0.1.0"
  license "MIT OR Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/keystone-db/keystonedb/releases/download/v#{version}/kstone-server-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_ACTUAL_SHA256_FOR_AARCH64_DARWIN"
    else
      url "https://github.com/keystone-db/keystonedb/releases/download/v#{version}/kstone-server-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_ACTUAL_SHA256_FOR_X86_64_DARWIN"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/keystone-db/keystonedb/releases/download/v#{version}/kstone-server-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_ACTUAL_SHA256_FOR_AARCH64_LINUX"
    else
      url "https://github.com/keystone-db/keystonedb/releases/download/v#{version}/kstone-server-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_ACTUAL_SHA256_FOR_X86_64_LINUX"
    end
  end

  def install
    bin.install "kstone-server"
  end

  service do
    run [opt_bin/"kstone-server", "--db-path", var/"kstone/db.keystone", "--host", "127.0.0.1", "--port", "50051"]
    keep_alive true
    log_path var/"log/kstone-server.log"
    error_log_path var/"log/kstone-server-error.log"
    working_dir var/"kstone"
  end

  def post_install
    # Create necessary directories
    (var/"kstone").mkpath
    (var/"log").mkpath
  end

  test do
    # Test that the binary runs and shows help
    assert_match "kstone-server", shell_output("#{bin}/kstone-server --help")

    # Test version flag
    system "#{bin}/kstone-server", "--version"
  end
end
