package main

import (
	"fmt"
	"log"
	"os"
	"path/filepath"

	kstone "github.com/keystone-db/keystonedb/bindings/go/embedded"
)

func main() {
	// Create a temporary database for this example
	tmpDir := os.TempDir()
	dbPath := filepath.Join(tmpDir, "example.keystone")

	// Clean up any existing database
	os.RemoveAll(dbPath)

	// Create database
	fmt.Println("Creating database at:", dbPath)
	db, err := kstone.Create(dbPath)
	if err != nil {
		log.Fatalf("Failed to create database: %v", err)
	}
	defer db.Close()

	fmt.Println("\n--- Example 1: Simple Put/Get/Delete ---")
	simpleCRUD(db)

	fmt.Println("\n--- Example 2: Using Sort Keys ---")
	sortKeyExample(db)

	fmt.Println("\n--- Example 3: Multiple Items in Partition ---")
	multipleItemsExample(db)

	fmt.Println("\n--- Example 4: Error Handling ---")
	errorHandlingExample(db)

	fmt.Println("\n✅ All examples completed successfully!")
	fmt.Println("Database location:", dbPath)
}

func simpleCRUD(db *kstone.Database) {
	// Put an item
	fmt.Println("Putting user#alice...")
	err := db.Put("user#alice", "name", "Alice Smith")
	if err != nil {
		log.Fatalf("Failed to put: %v", err)
	}

	err = db.Put("user#alice", "email", "alice@example.com")
	if err != nil {
		log.Fatalf("Failed to put email: %v", err)
	}

	err = db.Put("user#alice", "age", "30")
	if err != nil {
		log.Fatalf("Failed to put age: %v", err)
	}

	// Get the item
	fmt.Println("Getting user#alice...")
	item, err := db.Get("user#alice")
	if err != nil {
		log.Fatalf("Failed to get: %v", err)
	}

	fmt.Printf("Retrieved item: %+v\n", item)

	// Delete the item
	fmt.Println("Deleting user#alice...")
	err = db.Delete("user#alice")
	if err != nil {
		log.Fatalf("Failed to delete: %v", err)
	}

	// Verify deletion
	item, err = db.Get("user#alice")
	if err != kstone.ErrNotFound {
		log.Fatalf("Expected ErrNotFound, got: %v", err)
	}
	fmt.Println("Item successfully deleted (ErrNotFound returned)")
}

func sortKeyExample(db *kstone.Database) {
	// Put items with sort keys (organization hierarchy)
	fmt.Println("Creating organization hierarchy...")

	// Add users to organization
	err := db.PutWithSK("org#acme", "user#alice", "role", "admin")
	if err != nil {
		log.Fatalf("Failed to put: %v", err)
	}

	err = db.PutWithSK("org#acme", "user#bob", "role", "developer")
	if err != nil {
		log.Fatalf("Failed to put: %v", err)
	}

	err = db.PutWithSK("org#acme", "user#charlie", "role", "developer")
	if err != nil {
		log.Fatalf("Failed to put: %v", err)
	}

	// Get specific user
	fmt.Println("Getting org#acme/user#alice...")
	item, err := db.GetWithSK("org#acme", "user#alice")
	if err != nil {
		log.Fatalf("Failed to get: %v", err)
	}
	fmt.Printf("Retrieved: %+v\n", item)

	// Delete one user
	fmt.Println("Removing user#bob from org#acme...")
	err = db.DeleteWithSK("org#acme", "user#bob")
	if err != nil {
		log.Fatalf("Failed to delete: %v", err)
	}

	// Verify deletion
	_, err = db.GetWithSK("org#acme", "user#bob")
	if err != kstone.ErrNotFound {
		log.Fatalf("Expected ErrNotFound, got: %v", err)
	}
	fmt.Println("User successfully removed")
}

func multipleItemsExample(db *kstone.Database) {
	// Create multiple sensor readings
	fmt.Println("Creating sensor readings...")

	sensors := []struct {
		id    string
		value string
		unit  string
	}{
		{"sensor#001", "72.5", "fahrenheit"},
		{"sensor#002", "45.2", "celsius"},
		{"sensor#003", "1013.25", "hpa"},
	}

	for _, sensor := range sensors {
		err := db.Put(sensor.id, "value", sensor.value)
		if err != nil {
			log.Fatalf("Failed to put sensor %s: %v", sensor.id, err)
		}

		err = db.Put(sensor.id, "unit", sensor.unit)
		if err != nil {
			log.Fatalf("Failed to put unit for %s: %v", sensor.id, err)
		}

		fmt.Printf("Stored %s: %s %s\n", sensor.id, sensor.value, sensor.unit)
	}

	// Read them back
	fmt.Println("\nReading sensor data...")
	for _, sensor := range sensors {
		item, err := db.Get(sensor.id)
		if err != nil {
			log.Fatalf("Failed to get %s: %v", sensor.id, err)
		}
		fmt.Printf("%s: %+v\n", sensor.id, item)
	}
}

func errorHandlingExample(db *kstone.Database) {
	// Try to get non-existent item
	fmt.Println("Attempting to get non-existent item...")
	item, err := db.Get("does-not-exist")
	if err == kstone.ErrNotFound {
		fmt.Println("✓ Correctly received ErrNotFound")
	} else if err != nil {
		log.Fatalf("Unexpected error: %v", err)
	} else {
		log.Fatalf("Expected error, but got item: %+v", item)
	}

	// Try to get with sort key (non-existent)
	fmt.Println("Attempting to get non-existent item with sort key...")
	item, err = db.GetWithSK("no-such-pk", "no-such-sk")
	if err == kstone.ErrNotFound {
		fmt.Println("✓ Correctly received ErrNotFound")
	} else if err != nil {
		log.Fatalf("Unexpected error: %v", err)
	} else {
		log.Fatalf("Expected error, but got item: %+v", item)
	}
}
