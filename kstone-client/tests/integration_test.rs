/// Integration tests for KeystoneDB client
///
/// These tests start a real server and connect with the client to test
/// end-to-end functionality.

use kstone_api::Database;
use kstone_client::{Client, RemoteQuery, RemoteScan, RemoteBatchGetRequest, RemoteBatchWriteRequest};
use kstone_core::Value;
use kstone_server::{KeystoneDbServer, KeystoneService};
use std::collections::HashMap;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;
use tonic::transport::Server;

/// Helper to start a test server in the background
async fn start_test_server() -> (TempDir, String, tokio::task::JoinHandle<()>) {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();
    let service = KeystoneService::new(db);

    // Find an available port (using 0 lets OS choose)
    let addr = "127.0.0.1:0".parse().unwrap();
    let server = Server::builder()
        .add_service(KeystoneDbServer::new(service))
        .serve(addr);

    // Get the actual address the server bound to
    let actual_addr = format!("http://127.0.0.1:50051"); // For now, use fixed port

    let handle = tokio::spawn(async move {
        let addr = "127.0.0.1:50051".parse().unwrap();
        Server::builder()
            .add_service(KeystoneDbServer::new(KeystoneService::new(Database::create(TempDir::new().unwrap().path()).unwrap())))
            .serve(addr)
            .await
            .unwrap();
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    (dir, actual_addr, handle)
}

#[tokio::test]
async fn test_put_get_delete() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Create test item
    let mut item = HashMap::new();
    item.insert("name".to_string(), Value::S("Alice".to_string()));
    item.insert("age".to_string(), Value::N("30".to_string()));

    // Test put
    client.put(b"user#123", item.clone()).await.unwrap();

    // Test get
    let retrieved = client.get(b"user#123").await.unwrap();
    assert!(retrieved.is_some());
    let retrieved_item = retrieved.unwrap();
    assert_eq!(retrieved_item.get("name").unwrap(), &Value::S("Alice".to_string()));

    // Test delete
    client.delete(b"user#123").await.unwrap();

    // Verify deleted
    let after_delete = client.get(b"user#123").await.unwrap();
    assert!(after_delete.is_none());
}

#[tokio::test]
async fn test_put_get_with_sort_key() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    let mut item = HashMap::new();
    item.insert("name".to_string(), Value::S("Bob".to_string()));

    // Test put with sort key
    client.put_with_sk(b"org#1", b"user#456", item.clone()).await.unwrap();

    // Test get with sort key
    let retrieved = client.get_with_sk(b"org#1", b"user#456").await.unwrap();
    assert!(retrieved.is_some());

    // Test delete with sort key
    client.delete_with_sk(b"org#1", b"user#456").await.unwrap();

    let after_delete = client.get_with_sk(b"org#1", b"user#456").await.unwrap();
    assert!(after_delete.is_none());
}

#[tokio::test]
async fn test_query() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert test data
    for i in 1..=5 {
        let mut item = HashMap::new();
        item.insert("id".to_string(), Value::N(i.to_string()));
        client.put_with_sk(b"org#1", format!("user#{}", i).as_bytes(), item).await.unwrap();
    }

    // Query with begins_with
    let query = RemoteQuery::new(b"org#1")
        .sk_begins_with(b"user#")
        .limit(3);

    let response = client.query(query).await.unwrap();
    assert_eq!(response.count, 3);
    assert_eq!(response.items.len(), 3);
}

#[tokio::test]
async fn test_scan() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert test data
    for i in 1..=10 {
        let mut item = HashMap::new();
        item.insert("id".to_string(), Value::N(i.to_string()));
        client.put(format!("item#{}", i).as_bytes(), item).await.unwrap();
    }

    // Scan with limit
    let scan = RemoteScan::new()
        .limit(5);

    let response = client.scan(scan).await.unwrap();
    assert!(response.count <= 5);
}

#[tokio::test]
async fn test_batch_get() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert test data
    for i in 1..=3 {
        let mut item = HashMap::new();
        item.insert("id".to_string(), Value::N(i.to_string()));
        client.put(format!("batch#{}", i).as_bytes(), item).await.unwrap();
    }

    // Batch get
    let batch = RemoteBatchGetRequest::new()
        .add_key(b"batch#1")
        .add_key(b"batch#2")
        .add_key(b"batch#3");

    let response = client.batch_get(batch).await.unwrap();
    assert_eq!(response.count, 3);
}

#[tokio::test]
async fn test_batch_write() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Prepare items
    let mut item1 = HashMap::new();
    item1.insert("name".to_string(), Value::S("Item1".to_string()));

    let mut item2 = HashMap::new();
    item2.insert("name".to_string(), Value::S("Item2".to_string()));

    // Batch write
    let batch = RemoteBatchWriteRequest::new()
        .put(b"bw#1", item1)
        .put(b"bw#2", item2);

    let response = client.batch_write(batch).await.unwrap();
    assert!(response.success);

    // Verify items were written
    let item1_check = client.get(b"bw#1").await.unwrap();
    assert!(item1_check.is_some());
}
