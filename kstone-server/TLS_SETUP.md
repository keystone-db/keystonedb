# TLS/mTLS Setup Guide for KeystoneDB gRPC Server

This guide explains how to configure TLS (Transport Layer Security) and mTLS (mutual TLS) for the KeystoneDB gRPC server.

## Overview

The gRPC server supports three security modes:

1. **No TLS (default)**: Plaintext communication for development
2. **TLS**: Server authentication with certificate (encrypted communication)
3. **mTLS**: Mutual authentication - both server and client present certificates

## Generating Self-Signed Certificates (Development)

For development and testing, you can generate self-signed certificates using OpenSSL:

### 1. Generate CA Certificate (for mTLS)

```bash
# Generate CA private key
openssl genrsa -out ca-key.pem 4096

# Generate CA certificate
openssl req -new -x509 -days 365 -key ca-key.pem -out ca-cert.pem \
  -subj "/C=US/ST=State/L=City/O=KeystoneDB/CN=KeystoneDB CA"
```

### 2. Generate Server Certificate

```bash
# Generate server private key
openssl genrsa -out server-key.pem 4096

# Generate certificate signing request
openssl req -new -key server-key.pem -out server.csr \
  -subj "/C=US/ST=State/L=City/O=KeystoneDB/CN=localhost"

# Sign with CA (for mTLS) or self-sign (for TLS only)
# For mTLS:
openssl x509 -req -days 365 -in server.csr -CA ca-cert.pem -CAkey ca-key.pem \
  -CAcreateserial -out server-cert.pem

# For TLS only (self-signed):
openssl x509 -req -days 365 -in server.csr -signkey server-key.pem \
  -out server-cert.pem
```

### 3. Generate Client Certificate (for mTLS only)

```bash
# Generate client private key
openssl genrsa -out client-key.pem 4096

# Generate certificate signing request
openssl req -new -key client-key.pem -out client.csr \
  -subj "/C=US/ST=State/L=City/O=KeystoneDB/CN=client"

# Sign with CA
openssl x509 -req -days 365 -in client.csr -CA ca-cert.pem -CAkey ca-key.pem \
  -CAcreateserial -out client-cert.pem
```

## Running the Server

### Mode 1: No TLS (Development)

```bash
kstone-server --db-path ./mydb.keystone
```

Server runs on `http://127.0.0.1:50051` with plaintext communication.

### Mode 2: TLS (Server Authentication)

```bash
kstone-server \
  --db-path ./mydb.keystone \
  --tls-cert server-cert.pem \
  --tls-key server-key.pem
```

Server runs on `https://127.0.0.1:50051` with:
- Encrypted communication
- Server certificate verification
- No client authentication

### Mode 3: mTLS (Mutual Authentication)

```bash
kstone-server \
  --db-path ./mydb.keystone \
  --tls-cert server-cert.pem \
  --tls-key server-key.pem \
  --tls-ca ca-cert.pem
```

Server runs on `https://127.0.0.1:50051` with:
- Encrypted communication
- Server certificate verification
- **Client certificate verification** (clients must present valid certificates)

## Client Configuration

### Using kstone-client with TLS

```rust
use kstone_client::Client;
use tonic::transport::{Certificate, ClientTlsConfig, Identity};

// TLS (server auth only)
let ca_cert = std::fs::read_to_string("server-cert.pem")?;
let ca = Certificate::from_pem(ca_cert);
let tls = ClientTlsConfig::new().ca_certificate(ca);

let client = Client::connect_with_tls("https://localhost:50051", tls).await?;

// mTLS (mutual auth)
let ca_cert = std::fs::read_to_string("ca-cert.pem")?;
let client_cert = std::fs::read_to_string("client-cert.pem")?;
let client_key = std::fs::read_to_string("client-key.pem")?;

let ca = Certificate::from_pem(ca_cert);
let identity = Identity::from_pem(client_cert, client_key);

let tls = ClientTlsConfig::new()
    .ca_certificate(ca)
    .identity(identity);

let client = Client::connect_with_tls("https://localhost:50051", tls).await?;
```

## Production Certificates

For production environments, use certificates from a trusted Certificate Authority (CA):

1. **Let's Encrypt**: Free, automated certificates
2. **Commercial CA**: DigiCert, GlobalSign, etc.
3. **Private CA**: For internal infrastructure

### Let's Encrypt Example (with certbot)

```bash
# Install certbot
sudo apt-get install certbot

# Generate certificate
sudo certbot certonly --standalone -d yourdomain.com

# Certificates will be in:
# /etc/letsencrypt/live/yourdomain.com/fullchain.pem  (--tls-cert)
# /etc/letsencrypt/live/yourdomain.com/privkey.pem    (--tls-key)

# Run server
kstone-server \
  --db-path ./mydb.keystone \
  --host 0.0.0.0 \
  --port 443 \
  --tls-cert /etc/letsencrypt/live/yourdomain.com/fullchain.pem \
  --tls-key /etc/letsencrypt/live/yourdomain.com/privkey.pem
```

## Security Best Practices

1. **File Permissions**: Protect private keys
   ```bash
   chmod 600 server-key.pem client-key.pem ca-key.pem
   chmod 644 server-cert.pem client-cert.pem ca-cert.pem
   ```

2. **Key Storage**: Use secure key management
   - Hardware Security Modules (HSM) for production
   - Secret management services (Vault, AWS Secrets Manager)
   - Never commit keys to version control

3. **Certificate Rotation**: Regularly rotate certificates
   - Set appropriate expiration dates
   - Automate renewal (e.g., with Let's Encrypt)
   - Monitor certificate expiration

4. **Cipher Suites**: tonic/rustls uses secure defaults
   - TLS 1.2 and 1.3 only
   - Modern cipher suites
   - Perfect Forward Secrecy (PFS)

5. **Network Security**: Additional layers
   - Use firewall rules to restrict access
   - Consider running behind reverse proxy (nginx, envoy)
   - Enable rate limiting (built-in with --max-rps-* flags)

## Troubleshooting

### Certificate Validation Errors

**Error**: "TLS certificate file not found"
- **Solution**: Verify the file path is correct and file exists

**Error**: "Failed to read TLS certificate"
- **Solution**: Check file permissions and format (should be PEM format)

**Error**: "certificate verify failed"
- **Solution**: Ensure client trusts the server's CA, or add CA to trust store

### Connection Errors

**Error**: "connection refused"
- **Solution**: Check server is running and port is accessible

**Error**: "certificate signed by unknown authority"
- **Solution**: Client needs to trust the CA that signed the server certificate

### mTLS Errors

**Error**: "peer did not return a certificate"
- **Solution**: Client must present a certificate when mTLS is enabled

**Error**: "certificate required"
- **Solution**: Server requires client certificate - use mTLS configuration on client

## Validation

Verify TLS configuration with OpenSSL:

```bash
# Test TLS connection
openssl s_client -connect localhost:50051

# Test mTLS connection
openssl s_client -connect localhost:50051 \
  -cert client-cert.pem \
  -key client-key.pem \
  -CAfile ca-cert.pem

# Check certificate details
openssl x509 -in server-cert.pem -text -noout
```

## Example: Complete Setup

```bash
# 1. Generate certificates
./scripts/generate-certs.sh  # (create this script with commands from above)

# 2. Start server with mTLS
kstone-server \
  --db-path ./prod.keystone \
  --host 0.0.0.0 \
  --port 50051 \
  --tls-cert ./certs/server-cert.pem \
  --tls-key ./certs/server-key.pem \
  --tls-ca ./certs/ca-cert.pem \
  --max-connections 1000 \
  --connection-timeout 60 \
  --max-rps-global 10000

# 3. Test connection
grpcurl \
  -cert ./certs/client-cert.pem \
  -key ./certs/client-key.pem \
  -cacert ./certs/ca-cert.pem \
  localhost:50051 \
  list
```

## References

- [gRPC Authentication Guide](https://grpc.io/docs/guides/auth/)
- [Tonic TLS Documentation](https://docs.rs/tonic/latest/tonic/transport/struct.ServerTlsConfig.html)
- [OpenSSL Documentation](https://www.openssl.org/docs/)
- [Let's Encrypt](https://letsencrypt.org/)
