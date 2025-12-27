"""Type stubs for keystonedb Python bindings."""

from typing import Any, Dict, List, Optional, Union

Key = Union[bytes, str]
Item = Dict[str, Any]

class Database:
    """KeystoneDB database instance."""

    @staticmethod
    def create(path: str) -> "Database":
        """Create a new database at the given path."""
        ...

    @staticmethod
    def open(path: str) -> "Database":
        """Open an existing database at the given path."""
        ...

    @staticmethod
    def create_in_memory() -> "Database":
        """Create an in-memory database (no persistence)."""
        ...

    def put(
        self,
        pk: Key,
        item: Item,
        sk: Optional[Key] = None
    ) -> None:
        """Put an item into the database."""
        ...

    def get(
        self,
        pk: Key,
        sk: Optional[Key] = None
    ) -> Optional[Item]:
        """Get an item from the database."""
        ...

    def delete(
        self,
        pk: Key,
        sk: Optional[Key] = None
    ) -> None:
        """Delete an item from the database."""
        ...

    def query(
        self,
        pk: Key,
        *,
        sk_begins_with: Optional[Key] = None,
        sk_eq: Optional[Key] = None,
        sk_gt: Optional[Key] = None,
        sk_gte: Optional[Key] = None,
        sk_lt: Optional[Key] = None,
        sk_lte: Optional[Key] = None,
        limit: Optional[int] = None,
        forward: bool = True,
        index: Optional[str] = None
    ) -> List[Item]:
        """Query items by partition key with optional sort key conditions."""
        ...

    def scan(
        self,
        *,
        limit: Optional[int] = None,
        segment: Optional[int] = None,
        total_segments: Optional[int] = None
    ) -> List[Item]:
        """Scan all items in the database."""
        ...

    def update(
        self,
        pk: Key,
        expression: str,
        values: Optional[Dict[str, Any]] = None,
        sk: Optional[Key] = None,
        condition: Optional[str] = None
    ) -> Item:
        """Update an item using an update expression."""
        ...

    def execute(self, sql: str) -> Dict[str, Any]:
        """Execute a PartiQL/SQL statement."""
        ...

    def batch_get(self, keys: List[Key]) -> Dict[str, Item]:
        """Batch get multiple items."""
        ...

    def batch_write(
        self,
        puts: Optional[Dict[Key, Item]] = None,
        deletes: Optional[List[Key]] = None
    ) -> int:
        """Batch write multiple items."""
        ...

    def flush(self) -> None:
        """Flush pending writes to disk."""
        ...

class PyItemBuilder:
    """Fluent item builder."""

    def __init__(self) -> None: ...

    def string(self, key: str, value: str) -> "PyItemBuilder":
        """Add a string attribute."""
        ...

    def number(self, key: str, value: int) -> "PyItemBuilder":
        """Add a number attribute."""
        ...

    def bool(self, key: str, value: bool) -> "PyItemBuilder":
        """Add a boolean attribute."""
        ...

    def build(self) -> Item:
        """Build the item."""
        ...

def item_builder() -> PyItemBuilder:
    """Create a new ItemBuilder."""
    ...

def version() -> str:
    """Get the version of KeystoneDB."""
    ...
