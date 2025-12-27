"""Tests for KeystoneDB Python bindings."""

import pytest
import tempfile
import os

# Import will fail if the native module is not built
# Run `maturin develop` first to build and install the module
try:
    from keystonedb import Database, item_builder, version
except ImportError:
    pytest.skip("keystonedb module not built - run 'maturin develop'", allow_module_level=True)


class TestVersion:
    """Test version function."""

    def test_version(self):
        """Test that version returns a string."""
        v = version()
        assert isinstance(v, str)
        assert len(v) > 0


class TestInMemoryDatabase:
    """Test in-memory database operations."""

    def test_create_in_memory(self):
        """Test creating an in-memory database."""
        db = Database.create_in_memory()
        assert db is not None

    def test_put_get(self):
        """Test basic put and get operations."""
        db = Database.create_in_memory()

        # Put an item
        db.put(b"user#123", {"name": "Alice", "age": 30})

        # Get the item
        item = db.get(b"user#123")
        assert item is not None
        assert item["name"] == "Alice"
        assert item["age"] == 30

    def test_put_get_with_string_key(self):
        """Test put/get with string keys."""
        db = Database.create_in_memory()

        db.put("user#456", {"name": "Bob"})
        item = db.get("user#456")
        assert item is not None
        assert item["name"] == "Bob"

    def test_put_get_with_sort_key(self):
        """Test put/get with sort keys."""
        db = Database.create_in_memory()

        db.put(b"user#123", {"name": "Alice"}, sk=b"profile")
        item = db.get(b"user#123", sk=b"profile")
        assert item is not None
        assert item["name"] == "Alice"

    def test_get_not_found(self):
        """Test getting a non-existent item."""
        db = Database.create_in_memory()
        item = db.get(b"nonexistent")
        assert item is None

    def test_delete(self):
        """Test deleting an item."""
        db = Database.create_in_memory()

        db.put(b"user#123", {"name": "Alice"})
        assert db.get(b"user#123") is not None

        db.delete(b"user#123")
        assert db.get(b"user#123") is None

    def test_put_various_types(self):
        """Test storing various value types."""
        db = Database.create_in_memory()

        item = {
            "string": "hello",
            "number": 42,
            "float": 3.14,
            "bool": True,
            "null": None,
            "list": [1, 2, 3],
            "map": {"nested": "value"},
        }

        db.put(b"test#1", item)
        retrieved = db.get(b"test#1")

        assert retrieved["string"] == "hello"
        assert retrieved["number"] == 42
        assert abs(retrieved["float"] - 3.14) < 0.01
        assert retrieved["bool"] is True
        assert retrieved["null"] is None
        assert retrieved["list"] == [1, 2, 3]
        assert retrieved["map"]["nested"] == "value"


class TestItemBuilder:
    """Test ItemBuilder."""

    def test_item_builder(self):
        """Test building an item with ItemBuilder."""
        item = item_builder().string("name", "Alice").number("age", 30).bool("active", True).build()

        assert item["name"] == "Alice"
        assert item["age"] == 30
        assert item["active"] is True


class TestDiskDatabase:
    """Test disk-based database operations."""

    def test_create_and_open(self):
        """Test creating and reopening a database."""
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = os.path.join(tmpdir, "test.keystone")

            # Create and write
            db = Database.create(db_path)
            db.put(b"key1", {"value": "test"})
            db.flush()
            del db

            # Reopen and read
            db = Database.open(db_path)
            item = db.get(b"key1")
            assert item is not None
            assert item["value"] == "test"


class TestBatchOperations:
    """Test batch operations."""

    def test_batch_write_and_get(self):
        """Test batch write and batch get."""
        db = Database.create_in_memory()

        # Batch write
        puts = {
            b"user#1": {"name": "Alice"},
            b"user#2": {"name": "Bob"},
            b"user#3": {"name": "Charlie"},
        }
        count = db.batch_write(puts=puts)
        assert count == 3

        # Batch get
        results = db.batch_get([b"user#1", b"user#2", b"user#4"])
        assert "user#1" in results
        assert results["user#1"]["name"] == "Alice"
        assert "user#2" in results
        assert results["user#2"]["name"] == "Bob"
        # user#4 doesn't exist, should not be in results

    def test_batch_delete(self):
        """Test batch delete."""
        db = Database.create_in_memory()

        # Create items
        for i in range(5):
            db.put(f"item#{i}".encode(), {"id": i})

        # Delete some items
        count = db.batch_write(deletes=[b"item#1", b"item#3"])
        assert count == 2

        # Verify
        assert db.get(b"item#0") is not None
        assert db.get(b"item#1") is None
        assert db.get(b"item#2") is not None
        assert db.get(b"item#3") is None
        assert db.get(b"item#4") is not None


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
