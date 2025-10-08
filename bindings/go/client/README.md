# KeystoneDB Go gRPC Client

Go client library for connecting to a remote KeystoneDB server via gRPC.

## Installation

```bash
go get github.com/keystone-db/keystonedb/bindings/go/client
```

## Usage

### Basic Operations

```go
package main

import (
    "context"
    "fmt"
    "log"

    kstone "github.com/keystone-db/keystonedb/bindings/go/client"
    pb "github.com/keystone-db/keystonedb/bindings/go/client/pb"
)

func main() {
    // Connect to server
    client, err := kstone.Connect("localhost:50051")
    if err != nil {
        log.Fatal(err)
    }
    defer client.Close()

    ctx := context.Background()

    // Put an item
    putReq := kstone.NewPutRequest([]byte("user#123")).
        WithString("name", "Alice").
        WithNumber("age", "30").
        WithBool("active", true).
        Build()

    putResp, err := client.Put(ctx, putReq)
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Put success: %v\n", putResp.Success)

    // Get an item
    getReq := kstone.NewGetRequest([]byte("user#123")).Build()
    getResp, err := client.Get(ctx, getReq)
    if err != nil {
        log.Fatal(err)
    }

    if getResp.Item != nil {
        fmt.Printf("Got item: %v\n", getResp.Item)
    }

    // Query with sort key condition
    queryReq := kstone.NewQueryRequest([]byte("org#acme")).
        WithSortKeyBeginsWith(&pb.Value{
            Value: &pb.Value_StringValue{StringValue: "USER#"},
        }).
        WithLimit(10).
        Build()

    queryResp, err := client.Query(ctx, queryReq)
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Found %d items\n", queryResp.Count)

    // Scan all items
    scanReq := kstone.NewScanRequest().
        WithLimit(100).
        Build()

    items, err := client.Scan(ctx, scanReq)
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Scanned %d items\n", len(items))
}
```

### Batch Operations

```go
// Batch get
batchGetReq := &pb.BatchGetRequest{
    Keys: []*pb.Key{
        {PartitionKey: []byte("user#1")},
        {PartitionKey: []byte("user#2")},
    },
}
batchGetResp, err := client.BatchGet(ctx, batchGetReq)

// Batch write
batchWriteReq := &pb.BatchWriteRequest{
    Writes: []*pb.WriteRequest{
        {
            Request: &pb.WriteRequest_Put{
                Put: &pb.PutItem{
                    PartitionKey: []byte("user#1"),
                    Item: &pb.Item{
                        Attributes: map[string]*pb.Value{
                            "name": {Value: &pb.Value_StringValue{StringValue: "Alice"}},
                        },
                    },
                },
            },
        },
    },
}
batchWriteResp, err := client.BatchWrite(ctx, batchWriteReq)
```

## Features

- Connect to remote KeystoneDB server via gRPC
- Full support for CRUD operations (Put, Get, Delete)
- Query with sort key conditions
- Scan with streaming support
- Batch operations (BatchGet, BatchWrite)
- Transaction support (TransactGet, TransactWrite)
- Update expressions
- PartiQL query execution
- Builder pattern for common operations

## API Reference

### Client Methods

- `Connect(address string) (*Client, error)` - Connect to server
- `Close() error` - Close connection
- `Put(ctx, *PutRequest) (*PutResponse, error)` - Store item
- `Get(ctx, *GetRequest) (*GetResponse, error)` - Retrieve item
- `Delete(ctx, *DeleteRequest) (*DeleteResponse, error)` - Remove item
- `Query(ctx, *QueryRequest) (*QueryResponse, error)` - Query items
- `Scan(ctx, *ScanRequest) ([]*Item, error)` - Scan all items
- `BatchGet(ctx, *BatchGetRequest) (*BatchGetResponse, error)` - Get multiple items
- `BatchWrite(ctx, *BatchWriteRequest) (*BatchWriteResponse, error)` - Write multiple items
- `TransactGet(ctx, *TransactGetRequest) (*TransactGetResponse, error)` - Transactional get
- `TransactWrite(ctx, *TransactWriteRequest) (*TransactWriteResponse, error)` - Transactional write
- `Update(ctx, *UpdateRequest) (*UpdateResponse, error)` - Update item
- `ExecuteStatement(ctx, *ExecuteStatementRequest) (*ExecuteStatementResponse, error)` - Execute PartiQL

### Builders

- `NewPutRequest(pk []byte)` - Create put request builder
- `NewGetRequest(pk []byte)` - Create get request builder
- `NewQueryRequest(pk []byte)` - Create query request builder
- `NewScanRequest()` - Create scan request builder

## License

MIT OR Apache-2.0
