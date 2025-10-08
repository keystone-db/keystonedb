package client

import (
	"context"
	"fmt"
	"io"

	pb "github.com/keystone-db/keystonedb/bindings/go/client/pb"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

// Client wraps the gRPC client for KeystoneDB
type Client struct {
	conn   *grpc.ClientConn
	client pb.KeystoneDBClient
}

// Connect creates a new client connection to the KeystoneDB server
func Connect(address string) (*Client, error) {
	conn, err := grpc.NewClient(address, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, fmt.Errorf("failed to connect: %w", err)
	}

	return &Client{
		conn:   conn,
		client: pb.NewKeystoneDBClient(conn),
	}, nil
}

// Close closes the client connection
func (c *Client) Close() error {
	return c.conn.Close()
}

// Put stores an item in the database
func (c *Client) Put(ctx context.Context, req *pb.PutRequest) (*pb.PutResponse, error) {
	return c.client.Put(ctx, req)
}

// Get retrieves an item from the database
func (c *Client) Get(ctx context.Context, req *pb.GetRequest) (*pb.GetResponse, error) {
	return c.client.Get(ctx, req)
}

// Delete removes an item from the database
func (c *Client) Delete(ctx context.Context, req *pb.DeleteRequest) (*pb.DeleteResponse, error) {
	return c.client.Delete(ctx, req)
}

// Query performs a query operation
func (c *Client) Query(ctx context.Context, req *pb.QueryRequest) (*pb.QueryResponse, error) {
	return c.client.Query(ctx, req)
}

// Scan performs a scan operation with streaming
func (c *Client) Scan(ctx context.Context, req *pb.ScanRequest) ([]*pb.Item, error) {
	stream, err := c.client.Scan(ctx, req)
	if err != nil {
		return nil, fmt.Errorf("failed to start scan: %w", err)
	}

	var items []*pb.Item
	for {
		resp, err := stream.Recv()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("scan stream error: %w", err)
		}
		if resp.Error != nil && *resp.Error != "" {
			return nil, fmt.Errorf("scan error: %s", *resp.Error)
		}
		items = append(items, resp.Items...)
	}

	return items, nil
}

// BatchGet retrieves multiple items
func (c *Client) BatchGet(ctx context.Context, req *pb.BatchGetRequest) (*pb.BatchGetResponse, error) {
	return c.client.BatchGet(ctx, req)
}

// BatchWrite writes multiple items
func (c *Client) BatchWrite(ctx context.Context, req *pb.BatchWriteRequest) (*pb.BatchWriteResponse, error) {
	return c.client.BatchWrite(ctx, req)
}

// TransactGet performs a transactional get
func (c *Client) TransactGet(ctx context.Context, req *pb.TransactGetRequest) (*pb.TransactGetResponse, error) {
	return c.client.TransactGet(ctx, req)
}

// TransactWrite performs a transactional write
func (c *Client) TransactWrite(ctx context.Context, req *pb.TransactWriteRequest) (*pb.TransactWriteResponse, error) {
	return c.client.TransactWrite(ctx, req)
}

// Update updates an item
func (c *Client) Update(ctx context.Context, req *pb.UpdateRequest) (*pb.UpdateResponse, error) {
	return c.client.Update(ctx, req)
}

// ExecuteStatement executes a PartiQL statement
func (c *Client) ExecuteStatement(ctx context.Context, req *pb.ExecuteStatementRequest) (*pb.ExecuteStatementResponse, error) {
	return c.client.ExecuteStatement(ctx, req)
}
