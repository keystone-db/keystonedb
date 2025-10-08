#!/usr/bin/env python3
"""
KeystoneDB Python Example: Contact Manager

A simple CLI tool demonstrating KeystoneDB embedded Python bindings.
Manages a contact database with CRUD operations.
"""

import sys
import tempfile
from pathlib import Path
from typing import Optional

try:
    import keystonedb
except ImportError:
    print("Error: keystonedb package not found!")
    print("Install it with: pip install ../../bindings/python/embedded/target/wheels/keystonedb-*.whl")
    sys.exit(1)


class ContactManager:
    """Simple contact manager using KeystoneDB."""

    def __init__(self, db_path: str):
        """Initialize the contact manager."""
        self.db_path = db_path
        # Create or open database
        try:
            self.db = keystonedb.Database.open(db_path)
            print(f"📂 Opened existing database: {db_path}")
        except Exception:
            self.db = keystonedb.Database.create(db_path)
            print(f"📂 Created new database: {db_path}")

    def add_contact(self, name: str, email: str, phone: str, company: Optional[str] = None):
        """Add a new contact to the database."""
        contact_id = f"contact#{name.lower().replace(' ', '-')}"

        contact_data = {
            "name": name,
            "email": email,
            "phone": phone,
        }

        if company:
            contact_data["company"] = company

        try:
            self.db.put(contact_id.encode(), contact_data)
            print(f"✅ Added contact: {name}")
            return True
        except Exception as e:
            print(f"❌ Failed to add contact: {e}")
            return False

    def get_contact(self, name: str) -> Optional[dict]:
        """Retrieve a contact by name."""
        contact_id = f"contact#{name.lower().replace(' ', '-')}"

        try:
            contact = self.db.get(contact_id.encode())
            if contact:
                return contact
            else:
                print(f"⚠️  Contact not found: {name}")
                return None
        except Exception as e:
            print(f"❌ Error retrieving contact: {e}")
            return None

    def update_contact(self, name: str, **fields):
        """Update a contact's information."""
        contact_id = f"contact#{name.lower().replace(' ', '-')}"

        # Get existing contact
        existing = self.db.get(contact_id.encode())
        if not existing:
            print(f"⚠️  Contact not found: {name}")
            return False

        # Update fields
        for key, value in fields.items():
            if value is not None:
                existing[key] = value

        # Save updated contact
        try:
            self.db.put(contact_id.encode(), existing)
            print(f"✅ Updated contact: {name}")
            return True
        except Exception as e:
            print(f"❌ Failed to update contact: {e}")
            return False

    def delete_contact(self, name: str):
        """Delete a contact by name."""
        contact_id = f"contact#{name.lower().replace(' ', '-')}"

        try:
            self.db.delete(contact_id.encode())
            print(f"✅ Deleted contact: {name}")
            return True
        except Exception as e:
            print(f"❌ Failed to delete contact: {e}")
            return False

    def display_contact(self, contact: dict):
        """Pretty-print a contact."""
        print("─" * 50)
        for key, value in contact.items():
            print(f"  {key.capitalize():12} : {value}")
        print("─" * 50)

    def flush(self):
        """Flush database to disk."""
        try:
            self.db.flush()
            print("💾 Database flushed to disk")
        except Exception as e:
            print(f"❌ Failed to flush: {e}")


def demo_basic_operations():
    """Demonstrate basic CRUD operations."""
    print("\n" + "=" * 60)
    print("KeystoneDB Python Bindings Demo: Contact Manager")
    print("=" * 60)

    # Create temporary database
    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = str(Path(tmpdir) / "contacts.keystone")
        manager = ContactManager(db_path)

        print("\n--- Example 1: Adding Contacts ---")
        manager.add_contact(
            name="Alice Johnson",
            email="alice@example.com",
            phone="+1-555-0101",
            company="Acme Corp"
        )
        manager.add_contact(
            name="Bob Smith",
            email="bob@example.com",
            phone="+1-555-0102",
            company="Tech Innovations"
        )
        manager.add_contact(
            name="Charlie Brown",
            email="charlie@example.com",
            phone="+1-555-0103"
        )

        print("\n--- Example 2: Retrieving Contacts ---")
        contact = manager.get_contact("Alice Johnson")
        if contact:
            print("\nAlice Johnson's contact info:")
            manager.display_contact(contact)

        print("\n--- Example 3: Updating Contact ---")
        manager.update_contact(
            "Bob Smith",
            phone="+1-555-9999",
            company="New Startup Inc"
        )

        updated = manager.get_contact("Bob Smith")
        if updated:
            print("\nBob Smith's updated contact info:")
            manager.display_contact(updated)

        print("\n--- Example 4: Deleting Contact ---")
        manager.delete_contact("Charlie Brown")

        # Verify deletion
        deleted = manager.get_contact("Charlie Brown")
        if deleted is None:
            print("✓ Contact successfully deleted")

        print("\n--- Example 5: Persistence ---")
        manager.flush()
        print(f"Database saved at: {db_path}")

        print("\n--- Example 6: Value Types ---")
        demo_value_types(manager)

        print("\n✅ All examples completed successfully!")


def demo_value_types(manager: ContactManager):
    """Demonstrate different value types supported by KeystoneDB."""
    print("\nDemonstrating various value types...")

    complex_contact = {
        "name": "Diana Prince",
        "email": "diana@example.com",
        "phone": "+1-555-0104",
        "age": 30,
        "active": True,
        "department": None,  # Null value
        "tags": ["vip", "enterprise", "priority"],
        "metadata": {
            "created_at": "2024-01-01",
            "source": "web",
            "score": 95
        }
    }

    manager.db.put(b"contact#diana-prince", complex_contact)
    print("✅ Added contact with complex data types")

    retrieved = manager.db.get(b"contact#diana-prince")
    if retrieved:
        print("\nRetrieved contact with all value types:")
        manager.display_contact(retrieved)

        # Verify types
        print("\nValue type verification:")
        print(f"  name (str):       {isinstance(retrieved['name'], str)}")
        print(f"  age (int):        {isinstance(retrieved['age'], int)}")
        print(f"  active (bool):    {isinstance(retrieved['active'], bool)}")
        print(f"  department (None): {retrieved['department'] is None}")
        print(f"  tags (list):      {isinstance(retrieved['tags'], list)}")
        print(f"  metadata (dict):  {isinstance(retrieved['metadata'], dict)}")


def demo_sort_keys():
    """Demonstrate sort key functionality."""
    print("\n" + "=" * 60)
    print("KeystoneDB Python Bindings Demo: Sort Keys")
    print("=" * 60)

    with tempfile.TemporaryDirectory() as tmpdir:
        db_path = str(Path(tmpdir) / "org.keystone")
        db = keystonedb.Database.create(db_path)

        print("\n--- Organizing Contacts by Company ---")

        # Add employees to different companies
        companies = {
            "acme-corp": [
                ("alice", {"role": "CEO", "department": "Executive"}),
                ("bob", {"role": "CTO", "department": "Engineering"}),
            ],
            "tech-innovations": [
                ("charlie", {"role": "Developer", "department": "Engineering"}),
                ("diana", {"role": "Designer", "department": "Product"}),
            ]
        }

        for company, employees in companies.items():
            for emp_id, data in employees:
                pk = f"company#{company}".encode()
                sk = f"employee#{emp_id}".encode()
                db.put_with_sk(pk, sk, data)
                print(f"✅ Added {emp_id} to {company}")

        print("\n--- Retrieving Employees by Company ---")

        # Get specific employee
        employee = db.get_with_sk(b"company#acme-corp", b"employee#alice")
        if employee:
            print("\nAlice from Acme Corp:")
            print(f"  Role: {employee['role']}")
            print(f"  Department: {employee['department']}")

        # Delete employee
        print("\n--- Removing Employee ---")
        db.delete_with_sk(b"company#tech-innovations", b"employee#charlie")
        print("✅ Removed charlie from tech-innovations")

        # Verify deletion
        try:
            result = db.get_with_sk(b"company#tech-innovations", b"employee#charlie")
            if result is None:
                print("✓ Employee successfully removed")
        except Exception:
            print("✓ Employee successfully removed")

        print("\n✅ Sort key demo completed!")


def demo_in_memory():
    """Demonstrate in-memory database."""
    print("\n" + "=" * 60)
    print("KeystoneDB Python Bindings Demo: In-Memory Database")
    print("=" * 60)

    print("\n--- Creating In-Memory Database ---")
    db = keystonedb.Database.create_in_memory()
    print("✅ Created in-memory database (no disk I/O)")

    print("\n--- Storing Session Data ---")
    sessions = {
        "session#abc123": {"user_id": "user#1", "expires": 1704067200},
        "session#def456": {"user_id": "user#2", "expires": 1704153600},
        "session#ghi789": {"user_id": "user#3", "expires": 1704240000},
    }

    for session_id, data in sessions.items():
        db.put(session_id.encode(), data)
        print(f"✅ Stored {session_id}")

    print("\n--- Retrieving Session ---")
    session = db.get(b"session#abc123")
    if session:
        print(f"Session abc123: user={session['user_id']}, expires={session['expires']}")

    print("\n--- Cleanup ---")
    db.delete(b"session#abc123")
    print("✅ Session deleted")

    print("\n💡 Note: All data is in memory and will be lost when program exits")
    print("✅ In-memory demo completed!")


def main():
    """Run all demos."""
    if len(sys.argv) > 1 and sys.argv[1] == "--help":
        print(__doc__)
        print("\nUsage:")
        print("  python contacts.py              # Run all demos")
        print("  python contacts.py --help       # Show this help")
        return

    demo_basic_operations()
    demo_sort_keys()
    demo_in_memory()

    print("\n" + "=" * 60)
    print("🎉 All demos completed successfully!")
    print("=" * 60)


if __name__ == "__main__":
    main()
