"""
KeystoneDB Python bindings.

An embedded DynamoDB-compatible database for Python.

Example:
    >>> from keystonedb import Database
    >>> db = Database.create("mydb.keystone")
    >>> db.put(b"user#123", {"name": "Alice", "age": 30})
    >>> item = db.get(b"user#123")
    >>> print(item["name"])
    Alice
"""

# Import the native extension module
from .keystonedb import (
    Database,
    PyItemBuilder,
    item_builder,
    version,
)

__all__ = [
    "Database",
    "PyItemBuilder",
    "item_builder",
    "version",
]

__version__ = version()
