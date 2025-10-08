"""Smoke tests for Python embedded KeystoneDB bindings."""

import tempfile
import pathlib
import pytest


def test_smoke(tmp_path):
    """Basic create, put, get, delete test."""
    import keystonedb

    db_path = str(tmp_path / "test.keystone")

    # Create database
    db = keystonedb.Database.create(db_path)

    # Put an item
    db.put(b"user#123", {"name": "Alice", "age": 30})

    # Get the item back
    item = db.get(b"user#123")
    assert item is not None
    assert item["name"] == "Alice"
    assert item["age"] == 30

    # Delete the item
    db.delete(b"user#123")

    # Verify it's gone
    item = db.get(b"user#123")
    assert item is None


def test_smoke_with_sort_key(tmp_path):
    """Test with partition key and sort key."""
    import keystonedb

    db_path = str(tmp_path / "test.keystone")
    db = keystonedb.Database.create(db_path)

    # Put item with sort key
    db.put_with_sk(b"org#acme", b"user#123", {"role": "admin", "active": True})

    # Get item with sort key
    item = db.get_with_sk(b"org#acme", b"user#123")
    assert item is not None
    assert item["role"] == "admin"
    assert item["active"] is True

    # Delete with sort key
    db.delete_with_sk(b"org#acme", b"user#123")

    # Verify deletion
    item = db.get_with_sk(b"org#acme", b"user#123")
    assert item is None


def test_in_memory():
    """Test in-memory database."""
    import keystonedb

    # Create in-memory database
    db = keystonedb.Database.create_in_memory()

    # Put and get
    db.put(b"key1", {"value": "test", "count": 42})

    item = db.get(b"key1")
    assert item is not None
    assert item["value"] == "test"
    assert item["count"] == 42


def test_value_types(tmp_path):
    """Test different value types."""
    import keystonedb

    db_path = str(tmp_path / "test.keystone")
    db = keystonedb.Database.create(db_path)

    # Test various types
    db.put(b"test", {
        "string": "hello",
        "int": 42,
        "float": 3.14,
        "bool": True,
        "null": None,
        "list": [1, 2, "three"],
        "nested": {
            "inner": "value",
            "count": 10,
        },
    })

    item = db.get(b"test")
    assert item is not None
    assert item["string"] == "hello"
    assert item["int"] == 42
    assert item["float"] == 3.14
    assert item["bool"] is True
    assert item["null"] is None
    assert item["list"] == [1, 2, "three"]
    assert item["nested"]["inner"] == "value"
    assert item["nested"]["count"] == 10


def test_reopen(tmp_path):
    """Test that data persists across database open/close."""
    import keystonedb

    db_path = str(tmp_path / "test.keystone")

    # Create and write
    db = keystonedb.Database.create(db_path)
    db.put(b"persistent#1", {"data": "should persist"})
    db.flush()
    del db  # Close database

    # Reopen and verify data persisted
    db2 = keystonedb.Database.open(db_path)
    item = db2.get(b"persistent#1")
    assert item is not None
    assert item["data"] == "should persist"


def test_errors():
    """Test error handling."""
    import keystonedb

    # Try to open non-existent database
    with pytest.raises(Exception):
        keystonedb.Database.open("/nonexistent/path/db.keystone")

    # Try to create database in invalid location
    with pytest.raises(Exception):
        keystonedb.Database.create("/invalid\x00path/db.keystone")


def test_multiple_items(tmp_path):
    """Test multiple items in database."""
    import keystonedb

    db_path = str(tmp_path / "test.keystone")
    db = keystonedb.Database.create(db_path)

    # Put multiple items
    for i in range(10):
        db.put(f"item#{i}".encode(), {"id": i, "name": f"Item {i}"})

    # Verify all items
    for i in range(10):
        item = db.get(f"item#{i}".encode())
        assert item is not None
        assert item["id"] == i
        assert item["name"] == f"Item {i}"

    # Delete half
    for i in range(0, 10, 2):
        db.delete(f"item#{i}".encode())

    # Verify deletions
    for i in range(10):
        item = db.get(f"item#{i}".encode())
        if i % 2 == 0:
            assert item is None  # Should be deleted
        else:
            assert item is not None  # Should still exist


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
