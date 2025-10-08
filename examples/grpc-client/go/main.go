package main

import (
	"context"
	"fmt"
	"log"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	pb "github.com/keystone-db/keystonedb/bindings/go/client/pb"
)

const serverAddr = "localhost:50051"

func main() {
	// Connect to KeystoneDB server
	fmt.Printf("Connecting to KeystoneDB server at %s...\n", serverAddr)

	conn, err := grpc.NewClient(serverAddr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		log.Fatalf("Failed to connect: %v", err)
	}
	defer conn.Close()

	client := pb.NewKeystoneDbClient(conn)
	ctx := context.Background()

	fmt.Println("✅ Connected successfully!\n")

	// Run examples
	if err := createTasks(ctx, client); err != nil {
		log.Fatalf("Failed to create tasks: %v", err)
	}

	if err := getTask(ctx, client); err != nil {
		log.Fatalf("Failed to get task: %v", err)
	}

	if err := queryTasks(ctx, client); err != nil {
		log.Fatalf("Failed to query tasks: %v", err)
	}

	if err := batchOperations(ctx, client); err != nil {
		log.Fatalf("Failed to batch operations: %v", err)
	}

	if err := deleteTask(ctx, client); err != nil {
		log.Fatalf("Failed to delete task: %v", err)
	}

	fmt.Println("\n✅ All operations completed successfully!")
}

func createTasks(ctx context.Context, client pb.KeystoneDbClient) error {
	fmt.Println("--- Creating Tasks ---")

	tasks := []struct {
		id          string
		project     string
		title       string
		description string
		status      string
		priority    string
	}{
		{
			id:          "task#1",
			project:     "project#backend",
			title:       "Implement user authentication",
			description: "Add JWT-based auth system",
			status:      "in-progress",
			priority:    "high",
		},
		{
			id:          "task#2",
			project:     "project#backend",
			title:       "Set up database migrations",
			description: "Create migration scripts",
			status:      "pending",
			priority:    "medium",
		},
		{
			id:          "task#3",
			project:     "project#frontend",
			title:       "Design login page",
			description: "Create UI mockups",
			status:      "completed",
			priority:    "low",
		},
	}

	for _, task := range tasks {
		// Create task with simple key
		item := &pb.Item{
			Attributes: map[string]*pb.Value{
				"title":       {Kind: &pb.Value_S{S: task.title}},
				"description": {Kind: &pb.Value_S{S: task.description}},
				"status":      {Kind: &pb.Value_S{S: task.status}},
				"priority":    {Kind: &pb.Value_S{S: task.priority}},
				"created": {Kind: &pb.Value_N{N: fmt.Sprintf("%d",
					time.Now().Unix())}},
			},
		}

		// Put task by ID
		req := &pb.PutRequest{
			PartitionKey: []byte(task.id),
			Item:         item,
		}

		if _, err := client.Put(ctx, req); err != nil {
			return fmt.Errorf("failed to put %s: %w", task.id, err)
		}

		// Also put with project partition for querying
		reqProject := &pb.PutRequest{
			PartitionKey: []byte(task.project),
			SortKey:      []byte(task.id),
			Item:         item,
		}

		if _, err := client.Put(ctx, reqProject); err != nil {
			return fmt.Errorf("failed to put %s in %s: %w", task.id, task.project, err)
		}

		fmt.Printf("✅ Created task: %s\n", task.id)
	}

	return nil
}

func getTask(ctx context.Context, client pb.KeystoneDbClient) error {
	fmt.Println("\n--- Retrieving Task ---")

	req := &pb.GetRequest{
		PartitionKey: []byte("task#1"),
	}

	resp, err := client.Get(ctx, req)
	if err != nil {
		return fmt.Errorf("failed to get task: %w", err)
	}

	if resp.Item != nil {
		fmt.Println("Task task#1:")
		printItem(resp.Item)
	} else {
		fmt.Println("Task not found")
	}

	return nil
}

func queryTasks(ctx context.Context, client pb.KeystoneDbClient) error {
	fmt.Println("\n--- Querying Tasks by Project ---")

	// Query all tasks in project#backend
	req := &pb.QueryRequest{
		PartitionKey: []byte("project#backend"),
		Limit:        10,
	}

	resp, err := client.Query(ctx, req)
	if err != nil {
		return fmt.Errorf("failed to query: %w", err)
	}

	fmt.Printf("Found %d tasks for project#backend\n", len(resp.Items))

	for i, item := range resp.Items {
		fmt.Printf("\nTask %d:\n", i+1)
		printItem(item)
	}

	return nil
}

func batchOperations(ctx context.Context, client pb.KeystoneDbClient) error {
	fmt.Println("\n--- Batch Operations ---")

	// Batch get
	batchGetReq := &pb.BatchGetRequest{
		Keys: []*pb.Key{
			{PartitionKey: []byte("task#1")},
			{PartitionKey: []byte("task#2")},
		},
	}

	batchGetResp, err := client.BatchGet(ctx, batchGetReq)
	if err != nil {
		return fmt.Errorf("failed to batch get: %w", err)
	}

	fmt.Printf("Retrieved %d tasks in batch operation\n", len(batchGetResp.Items))

	return nil
}

func deleteTask(ctx context.Context, client pb.KeystoneDbClient) error {
	fmt.Println("\n--- Deleting Task ---")

	// Delete task#3
	req := &pb.DeleteRequest{
		PartitionKey: []byte("task#3"),
	}

	if _, err := client.Delete(ctx, req); err != nil {
		return fmt.Errorf("failed to delete task: %w", err)
	}

	// Also delete from project partition
	reqProject := &pb.DeleteRequest{
		PartitionKey: []byte("project#frontend"),
		SortKey:      []byte("task#3"),
	}

	if _, err := client.Delete(ctx, reqProject); err != nil {
		return fmt.Errorf("failed to delete task from project: %w", err)
	}

	fmt.Println("✅ Deleted task#3")

	return nil
}

func printItem(item *pb.Item) {
	for key, value := range item.Attributes {
		var valStr string
		switch v := value.Kind.(type) {
		case *pb.Value_S:
			valStr = v.S
		case *pb.Value_N:
			valStr = v.N
		case *pb.Value_Bool:
			valStr = fmt.Sprintf("%t", v.Bool)
		default:
			valStr = fmt.Sprintf("%v", value)
		}
		fmt.Printf("  %s: %s\n", key, valStr)
	}
}
