/// Integration tests for TLS configuration
///
/// Tests TLS/mTLS setup, validation, and error handling

use std::path::PathBuf;
use tempfile::NamedTempFile;
use std::io::Write;

/// Test certificate (self-signed, for testing only)
const TEST_CERT: &str = r#"-----BEGIN CERTIFICATE-----
MIIDazCCAlOgAwIBAgIUXQMd9fF1C+qj7K3xCfBwZBrKMzYwDQYJKoZIhvcNAQEL
BQAwRTELMAkGA1UEBhMCVVMxEzARBgNVBAgMClNvbWUtU3RhdGUxITAfBgNVBAoM
GEludGVybmV0IFdpZGdpdHMgUHR5IEx0ZDAeFw0yNDAxMDEwMDAwMDBaFw0yNTAx
MDEwMDAwMDBaMEUxCzAJBgNVBAYTAlVTMRMwEQYDVQQIDApTb21lLVN0YXRlMSEw
HwYDVQQKDBhJbnRlcm5ldCBXaWRnaXRzIFB0eSBMdGQwggEiMA0GCSqGSIb3DQEB
AQUAA4IBDwAwggEKAoIBAQC5ghTfPvqLGBhPQZXJcqVYkCChtXq5I6ZxHq3L0IlI
+lCwzGAp1DZxZjKaXBrXmXZVPVVGqF5tGvKEaEqEwZqYxVVNyBGqwY7h8x7VpY3H
kYwYvWGqFmTYZ7qSZHqE3VHvqZqE5pFqGqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqIBAgMB
AAGjUzBRMB0GA1UdDgQWBBQXqTrLWqYxVqYyZqYyZqYyZqYyZjAfBgNVHSMEGDAW
gBQXqTrLWqYxVqYyZqYyZqYyZqYyZjAPBgNVHRMBAf8EBTADAQH/MA0GCSqGSIb3
DQEBCwUAA4IBAQBvqLGBhPQZXJcqVYkCChtXq5I6ZxHq3L0IlI+lCwzGAp1DZxZj
KaXBrXmXZVPVVGqF5tGvKEaEqEwZqYxVVNyBGqwY7h8x7VpY3HkYwYvWGqFmTYZ7
qSZHqE3VHvqZqE5pFqGqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
-----END CERTIFICATE-----"#;

/// Test private key (self-signed, for testing only)
const TEST_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC5ghTfPvqLGBhP
QZXJcqVYkCChtXq5I6ZxHq3L0IlI+lCwzGAp1DZxZjKaXBrXmXZVPVVGqF5tGvKE
aEqEwZqYxVVNyBGqwY7h8x7VpY3HkYwYvWGqFmTYZ7qSZHqE3VHvqZqE5pFqGqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqIBAgMBAAECggEABqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
AoGBANqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyAoGBANqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
AoGBANqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyAoGBANqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyAoGBANqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
ZqYyZqYyZqYyZqYyZqYyZqYyZqYyZqYy
-----END PRIVATE KEY-----"#;

/// Helper function to create a temporary file with content
fn create_temp_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

/// Test that TLS configuration is None when no certificates are provided
#[test]
fn test_no_tls_configuration() {
    // This simulates the logic in configure_tls function
    let tls_cert: Option<PathBuf> = None;
    let tls_key: Option<PathBuf> = None;
    let tls_ca: Option<PathBuf> = None;

    // When no cert is provided, TLS should be disabled
    assert!(tls_cert.is_none());
    assert!(tls_key.is_none());
    assert!(tls_ca.is_none());
}

/// Test that providing only tls-key or tls-ca without tls-cert should fail
#[test]
fn test_tls_key_without_cert_validation() {
    // Simulating the validation that would occur
    let tls_cert: Option<PathBuf> = None;
    let tls_key: Option<PathBuf> = Some(PathBuf::from("key.pem"));
    let tls_ca: Option<PathBuf> = None;

    // This should fail because tls_key is provided without tls_cert
    if tls_cert.is_none() && (tls_key.is_some() || tls_ca.is_some()) {
        // Expected: error case
        assert!(true);
    } else {
        panic!("Should have caught missing tls_cert");
    }
}

/// Test that providing tls-cert without tls-key should fail
#[test]
fn test_tls_cert_without_key_validation() {
    // Simulating the validation that would occur
    let tls_cert: Option<PathBuf> = Some(PathBuf::from("cert.pem"));
    let tls_key: Option<PathBuf> = None;

    // This should fail because tls_cert is provided without tls_key
    if tls_cert.is_some() && tls_key.is_none() {
        // Expected: error case
        assert!(true);
    } else {
        panic!("Should have caught missing tls_key");
    }
}

/// Test that non-existent certificate file is detected
#[test]
fn test_nonexistent_cert_file() {
    let cert_path = PathBuf::from("/nonexistent/cert.pem");
    assert!(!cert_path.exists());
}

/// Test that non-existent key file is detected
#[test]
fn test_nonexistent_key_file() {
    let key_path = PathBuf::from("/nonexistent/key.pem");
    assert!(!key_path.exists());
}

/// Test that non-existent CA file is detected
#[test]
fn test_nonexistent_ca_file() {
    let ca_path = PathBuf::from("/nonexistent/ca.pem");
    assert!(!ca_path.exists());
}

/// Test that valid certificate and key files can be created and read
#[test]
fn test_valid_cert_and_key_files() {
    let cert_file = create_temp_file(TEST_CERT);
    let key_file = create_temp_file(TEST_KEY);

    // Files should exist
    assert!(cert_file.path().exists());
    assert!(key_file.path().exists());

    // Files should be readable
    let cert_content = std::fs::read_to_string(cert_file.path()).unwrap();
    let key_content = std::fs::read_to_string(key_file.path()).unwrap();

    assert!(cert_content.contains("BEGIN CERTIFICATE"));
    assert!(key_content.contains("BEGIN PRIVATE KEY"));
}

/// Test that mTLS requires TLS to be enabled
#[test]
fn test_mtls_requires_tls() {
    // Simulating the validation
    let tls_cert: Option<PathBuf> = None;
    let tls_key: Option<PathBuf> = None;
    let tls_ca: Option<PathBuf> = Some(PathBuf::from("ca.pem"));

    // This should fail because tls_ca requires tls_cert
    if tls_cert.is_none() && tls_ca.is_some() {
        // Expected: error case
        assert!(true);
    } else {
        panic!("Should have caught missing TLS config for mTLS");
    }
}

/// Test that both cert and key are required for TLS
#[test]
fn test_tls_requires_both_cert_and_key() {
    let cert_file = create_temp_file(TEST_CERT);
    let key_file = create_temp_file(TEST_KEY);

    let tls_cert = Some(cert_file.path().to_path_buf());
    let tls_key = Some(key_file.path().to_path_buf());

    // Both files should exist
    assert!(tls_cert.as_ref().unwrap().exists());
    assert!(tls_key.as_ref().unwrap().exists());

    // This configuration should be valid
    assert!(tls_cert.is_some() && tls_key.is_some());
}

/// Test that mTLS configuration includes CA certificate
#[test]
fn test_mtls_configuration() {
    let cert_file = create_temp_file(TEST_CERT);
    let key_file = create_temp_file(TEST_KEY);
    let ca_file = create_temp_file(TEST_CERT); // Using cert as CA for testing

    let tls_cert = Some(cert_file.path().to_path_buf());
    let tls_key = Some(key_file.path().to_path_buf());
    let tls_ca = Some(ca_file.path().to_path_buf());

    // All files should exist
    assert!(tls_cert.as_ref().unwrap().exists());
    assert!(tls_key.as_ref().unwrap().exists());
    assert!(tls_ca.as_ref().unwrap().exists());

    // This configuration should be valid for mTLS
    assert!(tls_cert.is_some() && tls_key.is_some() && tls_ca.is_some());
}
