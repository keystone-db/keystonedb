# KeystoneDB CLI Homebrew Formula
#
# Installation:
#   brew tap keystone-db/keystonedb
#   brew install kstone
#
# Note: Update SHA256 checksums after each release

class Kstone < Formula
  desc "Single-file, embedded, DynamoDB-style database CLI"
  homepage "https://github.com/keystone-db/keystonedb"
  version "0.1.0"
  license "MIT OR Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/keystone-db/keystonedb/releases/download/v#{version}/kstone-aarch64-apple-darwin.tar.gz"
      sha256 "69eedf9cbcb83978a14566934ad378d4bef0b6b007bf0b645cf5e752eddba49c"
    else
      url "https://github.com/keystone-db/keystonedb/releases/download/v#{version}/kstone-x86_64-apple-darwin.tar.gz"
      sha256 "8d27008932c1c888c6a577f70673768bde5c03dc6ced491bef9cf1bf925cb3fe"
    end
  end

  on_linux do
    # ARM64 Linux build not available in v0.1.0
    # if Hardware::CPU.arm?
    #   url "https://github.com/keystone-db/keystonedb/releases/download/v#{version}/kstone-aarch64-unknown-linux-gnu.tar.gz"
    #   sha256 "REPLACE_WITH_ACTUAL_SHA256_FOR_AARCH64_LINUX"
    # else
      url "https://github.com/keystone-db/keystonedb/releases/download/v#{version}/kstone-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "b0a25e8c1a284a194f620afe5909ca8b639f73c4135f56827e74766595cd1d46"
    # end
  end

  def install
    bin.install "kstone"
  end

  test do
    # Test that the binary runs and shows version
    assert_match "kstone", shell_output("#{bin}/kstone --version")

    # Create a test database
    system "#{bin}/kstone", "create", "test.keystone"
    assert_predicate testpath/"test.keystone", :exist?

    # Put an item
    system "#{bin}/kstone", "put", "test.keystone", "test#1", '{"name":"test"}'

    # Get the item back
    output = shell_output("#{bin}/kstone get test.keystone test#1")
    assert_match "test", output

    # Clean up
    rm_rf "test.keystone"
  end
end
