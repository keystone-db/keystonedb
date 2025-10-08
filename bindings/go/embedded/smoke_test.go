package keystone_test

import (
	"path/filepath"
	"testing"

	kstone "github.com/keystone-db/keystonedb/bindings/go/embedded"
)

func TestSmoke(t *testing.T) {
	// Create temp directory for test database
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.keystone")

	// Create database
	db, err := kstone.Create(dbPath)
	if err != nil {
		t.Fatalf("Failed to create database: %v", err)
	}
	defer db.Close()

	// Put an item
	err = db.Put("user#123", "name", "Alice")
	if err != nil {
		t.Fatalf("Failed to put item: %v", err)
	}

	// Get the item back
	item, err := db.Get("user#123")
	if err != nil {
		t.Fatalf("Failed to get item: %v", err)
	}
	if item == nil {
		t.Fatal("Expected item, got nil")
	}

	// Delete the item
	err = db.Delete("user#123")
	if err != nil {
		t.Fatalf("Failed to delete item: %v", err)
	}

	// Verify it's gone
	item, err = db.Get("user#123")
	if err != kstone.ErrNotFound {
		t.Fatalf("Expected ErrNotFound, got: %v", err)
	}
}

func TestSmokeWithSortKey(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.keystone")

	db, err := kstone.Create(dbPath)
	if err != nil {
		t.Fatalf("Failed to create database: %v", err)
	}
	defer db.Close()

	// Put item with sort key
	err = db.PutWithSK("org#acme", "user#123", "role", "admin")
	if err != nil {
		t.Fatalf("Failed to put item with SK: %v", err)
	}

	// Get item with sort key
	item, err := db.GetWithSK("org#acme", "user#123")
	if err != nil {
		t.Fatalf("Failed to get item with SK: %v", err)
	}
	if item == nil {
		t.Fatal("Expected item, got nil")
	}

	// Delete with sort key
	err = db.DeleteWithSK("org#acme", "user#123")
	if err != nil {
		t.Fatalf("Failed to delete item with SK: %v", err)
	}

	// Verify deletion
	item, err = db.GetWithSK("org#acme", "user#123")
	if err != kstone.ErrNotFound {
		t.Fatalf("Expected ErrNotFound, got: %v", err)
	}
}

func TestInMemory(t *testing.T) {
	// Create in-memory database
	db, err := kstone.CreateInMemory()
	if err != nil {
		t.Fatalf("Failed to create in-memory database: %v", err)
	}
	defer db.Close()

	// Put and get
	err = db.Put("key1", "value", "test")
	if err != nil {
		t.Fatalf("Failed to put: %v", err)
	}

	item, err := db.Get("key1")
	if err != nil {
		t.Fatalf("Failed to get: %v", err)
	}
	if item == nil {
		t.Fatal("Expected item, got nil")
	}
}

func TestReopen(t *testing.T) {
	tmpDir := t.TempDir()
	dbPath := filepath.Join(tmpDir, "test.keystone")

	// Create and write
	db, err := kstone.Create(dbPath)
	if err != nil {
		t.Fatalf("Failed to create database: %v", err)
	}

	err = db.Put("persistent#1", "data", "should persist")
	if err != nil {
		t.Fatalf("Failed to put: %v", err)
	}

	err = db.Close()
	if err != nil {
		t.Fatalf("Failed to close: %v", err)
	}

	// Reopen and verify data persisted
	db2, err := kstone.Open(dbPath)
	if err != nil {
		t.Fatalf("Failed to reopen database: %v", err)
	}
	defer db2.Close()

	item, err := db2.Get("persistent#1")
	if err != nil {
		t.Fatalf("Failed to get after reopen: %v", err)
	}
	if item == nil {
		t.Fatal("Expected item to persist, got nil")
	}
}

func TestErrors(t *testing.T) {
	// Try to open non-existent database
	_, err := kstone.Open("/nonexistent/path/db.keystone")
	if err == nil {
		t.Fatal("Expected error opening non-existent database")
	}

	// Try to create database in invalid location
	_, err = kstone.Create("/invalid\x00path/db.keystone")
	if err == nil {
		t.Fatal("Expected error creating database at invalid path")
	}
}
