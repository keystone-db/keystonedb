/// Integration tests for KeystoneDB client
///
/// These tests start a real server and connect with the client to test
/// end-to-end functionality.

use kstone_api::Database;
use kstone_client::{
    Client, RemoteQuery, RemoteScan, RemoteBatchGetRequest, RemoteBatchWriteRequest,
    RemoteTransactGetRequest, RemoteTransactWriteRequest, RemoteUpdate,
    RemoteExecuteStatementResponse
};
use kstone_core::Value;
use kstone_server::{KeystoneDbServer, KeystoneService};
use std::collections::HashMap;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;
use tonic::transport::Server;

/// Helper to start a test server in the background
async fn start_test_server() -> (TempDir, String, tokio::task::JoinHandle<()>) {
    use std::net::TcpListener;

    // Create database
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();
    let service = KeystoneService::new(db);

    // Find an available port by binding to port 0
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener); // Release the port for the server to use

    let addr_str = format!("127.0.0.1:{}", port);
    let client_addr = format!("http://{}", addr_str);

    // Spawn server in background
    let handle = tokio::spawn(async move {
        let addr = addr_str.parse().unwrap();
        Server::builder()
            .add_service(KeystoneDbServer::new(service))
            .serve(addr)
            .await
            .unwrap();
    });

    // Give server time to start
    sleep(Duration::from_millis(200)).await;

    (dir, client_addr, handle)
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

#[tokio::test]
async fn test_transact_get() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert test data
    let mut item1 = HashMap::new();
    item1.insert("name".to_string(), Value::S("Alice".to_string()));
    client.put(b"user#1", item1).await.unwrap();

    let mut item2 = HashMap::new();
    item2.insert("name".to_string(), Value::S("Bob".to_string()));
    client.put(b"user#2", item2).await.unwrap();

    // Transact get
    let request = RemoteTransactGetRequest::new()
        .get(b"user#1")
        .get(b"user#2")
        .get(b"user#999"); // Non-existent

    let response = client.transact_get(request).await.unwrap();

    // Verify results
    assert_eq!(response.items.len(), 3);
    assert!(response.items[0].is_some());
    assert!(response.items[1].is_some());
    assert!(response.items[2].is_none());
}

#[tokio::test]
async fn test_transact_write() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Prepare items
    let mut item1 = HashMap::new();
    item1.insert("balance".to_string(), Value::N("100".to_string()));

    let mut item2 = HashMap::new();
    item2.insert("balance".to_string(), Value::N("50".to_string()));

    // Transact write - transfer money between accounts
    let request = RemoteTransactWriteRequest::new()
        .put(b"account#1", item1)
        .put(b"account#2", item2);

    client.transact_write(request).await.unwrap();

    // Verify both items were written
    let acc1 = client.get(b"account#1").await.unwrap();
    assert!(acc1.is_some());

    let acc2 = client.get(b"account#2").await.unwrap();
    assert!(acc2.is_some());
}

#[tokio::test]
async fn test_update_operation() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert initial item
    let mut item = HashMap::new();
    item.insert("counter".to_string(), Value::N("0".to_string()));
    item.insert("name".to_string(), Value::S("Test".to_string()));
    client.put(b"item#1", item).await.unwrap();

    // Update - increment counter
    let update = RemoteUpdate::new(b"item#1")
        .expression("SET counter = counter + :inc")
        .value(":inc", Value::N("5".to_string()));

    let response = client.update(update).await.unwrap();

    // Verify update
    match response.item.get("counter").unwrap() {
        Value::N(n) => assert_eq!(n, "5"),
        _ => panic!("Expected number"),
    }
}

#[tokio::test]
async fn test_execute_statement_select() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert test data
    let mut item = HashMap::new();
    item.insert("name".to_string(), Value::S("Alice".to_string()));
    item.insert("age".to_string(), Value::N("30".to_string()));
    client.put(b"user#123", item).await.unwrap();

    // Execute SELECT statement
    let response = client
        .execute_statement("SELECT * FROM users WHERE pk = 'user#123'")
        .await
        .unwrap();

    // Verify response
    match response {
        RemoteExecuteStatementResponse::Select { items, count, .. } => {
            assert_eq!(count, 1);
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0].get("name").unwrap(),
                &Value::S("Alice".to_string())
            );
        }
        _ => panic!("Expected SELECT response"),
    }
}

#[tokio::test]
async fn test_execute_statement_insert() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Execute INSERT statement
    let response = client
        .execute_statement(
            "INSERT INTO users VALUE {'pk': 'user#456', 'name': 'Bob', 'age': 25}"
        )
        .await
        .unwrap();

    // Verify response
    match response {
        RemoteExecuteStatementResponse::Insert { success } => {
            assert!(success);
        }
        _ => panic!("Expected INSERT response"),
    }

    // Verify item was inserted
    let item = client.get(b"user#456").await.unwrap();
    assert!(item.is_some());
}

#[tokio::test]
async fn test_execute_statement_update() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert initial item
    let mut item = HashMap::new();
    item.insert("score".to_string(), Value::N("100".to_string()));
    client.put(b"game#1", item).await.unwrap();

    // Execute UPDATE statement
    let response = client
        .execute_statement("UPDATE games SET score = score + 50 WHERE pk = 'game#1'")
        .await
        .unwrap();

    // Verify response
    match response {
        RemoteExecuteStatementResponse::Update { item } => {
            match item.get("score").unwrap() {
                Value::N(n) => assert_eq!(n, "150"),
                _ => panic!("Expected number"),
            }
        }
        _ => panic!("Expected UPDATE response"),
    }
}

#[tokio::test]
async fn test_execute_statement_delete() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert item
    let mut item = HashMap::new();
    item.insert("temp".to_string(), Value::S("data".to_string()));
    client.put(b"temp#1", item).await.unwrap();

    // Execute DELETE statement
    let response = client
        .execute_statement("DELETE FROM items WHERE pk = 'temp#1'")
        .await
        .unwrap();

    // Verify response
    match response {
        RemoteExecuteStatementResponse::Delete { success } => {
            assert!(success);
        }
        _ => panic!("Expected DELETE response"),
    }

    // Verify item was deleted
    let item = client.get(b"temp#1").await.unwrap();
    assert!(item.is_none());
}

#[tokio::test]
async fn test_conditional_put_success() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // First put should succeed (name attribute doesn't exist yet)
    let mut item = HashMap::new();
    item.insert("name".to_string(), Value::S("Alice".to_string()));

    client
        .put_conditional(
            b"user#1",
            item.clone(),
            "attribute_not_exists(name)",
            HashMap::new(),
        )
        .await
        .unwrap();

    // Verify item was created
    let retrieved = client.get(b"user#1").await.unwrap();
    assert!(retrieved.is_some());
}

#[tokio::test]
async fn test_conditional_put_failure() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert initial item
    let mut item = HashMap::new();
    item.insert("name".to_string(), Value::S("Alice".to_string()));
    client.put(b"user#1", item.clone()).await.unwrap();

    // Second put should fail (name attribute already exists)
    let result = client
        .put_conditional(
            b"user#1",
            item,
            "attribute_not_exists(name)",
            HashMap::new(),
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_conditional_delete_success() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert item
    let mut item = HashMap::new();
    item.insert("status".to_string(), Value::S("active".to_string()));
    client.put(b"user#1", item).await.unwrap();

    // Delete with condition (should succeed)
    let mut values = HashMap::new();
    values.insert(":status".to_string(), Value::S("active".to_string()));

    client
        .delete_conditional(b"user#1", "status = :status", values)
        .await
        .unwrap();

    // Verify item was deleted
    let result = client.get(b"user#1").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_conditional_delete_failure() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert item
    let mut item = HashMap::new();
    item.insert("status".to_string(), Value::S("active".to_string()));
    client.put(b"user#1", item).await.unwrap();

    // Delete with wrong condition (should fail)
    let mut values = HashMap::new();
    values.insert(":status".to_string(), Value::S("inactive".to_string()));

    let result = client
        .delete_conditional(b"user#1", "status = :status", values)
        .await;

    assert!(result.is_err());

    // Verify item was NOT deleted
    let retrieved = client.get(b"user#1").await.unwrap();
    assert!(retrieved.is_some());
}

#[tokio::test]
async fn test_concurrent_clients() {
    let (_dir, addr, _handle) = start_test_server().await;

    // Spawn multiple clients concurrently
    let mut handles = vec![];

    for i in 0..5 {
        let addr_clone = addr.clone();
        let handle = tokio::spawn(async move {
            let mut client = Client::connect(addr_clone).await.unwrap();

            // Each client writes its own items
            for j in 0..10 {
                let key = format!("client{}#item{}", i, j);
                let mut item = HashMap::new();
                item.insert("client_id".to_string(), Value::N(i.to_string()));
                item.insert("item_id".to_string(), Value::N(j.to_string()));

                client.put(key.as_bytes(), item).await.unwrap();
            }

            // Read back and verify
            for j in 0..10 {
                let key = format!("client{}#item{}", i, j);
                let retrieved = client.get(key.as_bytes()).await.unwrap();
                assert!(retrieved.is_some());
            }
        });

        handles.push(handle);
    }

    // Wait for all clients to complete
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_concurrent_writes_same_key() {
    let (_dir, addr, _handle) = start_test_server().await;

    // Multiple clients writing to the same key
    let mut handles = vec![];

    for i in 0..10 {
        let addr_clone = addr.clone();
        let handle = tokio::spawn(async move {
            let mut client = Client::connect(addr_clone).await.unwrap();

            let mut item = HashMap::new();
            item.insert("counter".to_string(), Value::N(i.to_string()));

            client.put(b"shared#key", item).await.unwrap();
        });

        handles.push(handle);
    }

    // Wait for all writes
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify the key exists (one of the writes succeeded)
    let mut client = Client::connect(addr).await.unwrap();
    let result = client.get(b"shared#key").await.unwrap();
    assert!(result.is_some());
}

#[tokio::test]
async fn test_connection_refused() {
    // Try to connect to non-existent server
    let result = Client::connect("http://127.0.0.1:9999").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_nonexistent_item() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Get item that doesn't exist
    let result = client.get(b"nonexistent#key").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_query_nonexistent_partition() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Query partition that has no items
    let query = RemoteQuery::new(b"nonexistent#partition");
    let response = client.query(query).await.unwrap();

    assert_eq!(response.count, 0);
    assert_eq!(response.items.len(), 0);
}

#[tokio::test]
async fn test_invalid_partiql_statement() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Execute invalid SQL
    let result = client
        .execute_statement("INVALID SQL STATEMENT")
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_nonexistent_item() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Try to update item that doesn't exist
    let update = RemoteUpdate::new(b"nonexistent#item")
        .expression("SET counter = counter + :inc")
        .value(":inc", Value::N("1".to_string()));

    let result = client.update(update).await;

    // Should fail because item doesn't exist
    assert!(result.is_err());
}

#[tokio::test]
async fn test_transact_get_with_missing_items() {
    let (_dir, addr, _handle) = start_test_server().await;
    let mut client = Client::connect(addr).await.unwrap();

    // Insert only one item
    let mut item = HashMap::new();
    item.insert("name".to_string(), Value::S("Alice".to_string()));
    client.put(b"user#1", item).await.unwrap();

    // Transact get with mix of existing and non-existing
    let request = RemoteTransactGetRequest::new()
        .get(b"user#1")
        .get(b"user#2")
        .get(b"user#3");

    let response = client.transact_get(request).await.unwrap();

    // Should get back 3 results, with 2 being None
    assert_eq!(response.items.len(), 3);
    assert!(response.items[0].is_some());
    assert!(response.items[1].is_none());
    assert!(response.items[2].is_none());
}
