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
      sha256 "a265512f08dc38b9a1d6686e660e55af0b3edf91d427393b21472f34e17e1747"
    else
      url "https://github.com/keystone-db/keystonedb/releases/download/v#{version}/kstone-server-x86_64-apple-darwin.tar.gz"
      sha256 "5f400b8ad114b86be01c6b7667960b17157f6dd475766f472e72f9e85cabf03c"
    end
  end

  on_linux do
    # ARM64 Linux build not available in v0.1.0
    # if Hardware::CPU.arm?
    #   url "https://github.com/keystone-db/keystonedb/releases/download/v#{version}/kstone-server-aarch64-unknown-linux-gnu.tar.gz"
    #   sha256 "REPLACE_WITH_ACTUAL_SHA256_FOR_AARCH64_LINUX"
    # else
      url "https://github.com/keystone-db/keystonedb/releases/download/v#{version}/kstone-server-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e7f82b8859beb872b5bb6c18a12c8ad18acae84b69bdef750655312d63139c92"
    # end
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
