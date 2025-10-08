#!/usr/bin/env python3
"""
KeystoneDB gRPC Python Client Example: Task Management

Demonstrates remote database operations using the Python gRPC client.
"""

import sys
import time
from pathlib import Path

# Add bindings to path
sys.path.insert(0, str(Path(__file__).parent / "../../../bindings/python/client"))

try:
    from keystonedb import Client
    from keystonedb.builders import PutRequestBuilder, GetRequestBuilder, QueryRequestBuilder
except ImportError as e:
    print(f"Error importing keystonedb client: {e}")
    print("\nMake sure you've generated the protobuf files:")
    print("  python -m grpc_tools.protoc \\")
    print("    --python_out=bindings/python/client/keystonedb \\")
    print("    --grpc_python_out=bindings/python/client/keystonedb \\")
    print("    --proto_path=kstone-proto/proto \\")
    print("    kstone-proto/proto/keystone.proto")
    sys.exit(1)


SERVER_ADDR = "localhost:50051"


async def create_tasks(client: Client):
    """Create sample tasks in the database."""
    print("--- Creating Tasks ---")

    tasks = [
        {
            "id": "task#1",
            "project": "project#backend",
            "title": "Implement user authentication",
            "description": "Add JWT-based auth system",
            "status": "in-progress",
            "priority": "high",
        },
        {
            "id": "task#2",
            "project": "project#backend",
            "title": "Set up database migrations",
            "description": "Create migration scripts",
            "status": "pending",
            "priority": "medium",
        },
        {
            "id": "task#3",
            "project": "project#frontend",
            "title": "Design login page",
            "description": "Create UI mockups",
            "status": "completed",
            "priority": "low",
        },
    ]

    for task in tasks:
        # Build item attributes
        attributes = {
            "title": {"S": task["title"]},
            "description": {"S": task["description"]},
            "status": {"S": task["status"]},
            "priority": {"S": task["priority"]},
            "created": {"N": str(int(time.time()))},
        }

        # Put task by ID
        request = (
            PutRequestBuilder()
            .partition_key(task["id"].encode())
            .attributes(attributes)
            .build()
        )
        await client.put(request)

        # Also put with project partition for querying
        request_project = (
            PutRequestBuilder()
            .partition_key(task["project"].encode())
            .sort_key(task["id"].encode())
            .attributes(attributes)
            .build()
        )
        await client.put(request_project)

        print(f"✅ Created task: {task['id']}")


async def get_task(client: Client):
    """Retrieve a specific task."""
    print("\n--- Retrieving Task ---")

    request = GetRequestBuilder().partition_key(b"task#1").build()

    response = await client.get(request)

    if response.item:
        print("Task task#1:")
        print_item(response.item)
    else:
        print("Task not found")


async def query_tasks(client: Client):
    """Query tasks by project."""
    print("\n--- Querying Tasks by Project ---")

    request = (
        QueryRequestBuilder()
        .partition_key(b"project#backend")
        .limit(10)
        .build()
    )

    response = await client.query(request)

    print(f"Found {len(response.items)} tasks for project#backend")

    for i, item in enumerate(response.items, 1):
        print(f"\nTask {i}:")
        print_item(item)


async def batch_operations(client: Client):
    """Demonstrate batch get operation."""
    print("\n--- Batch Operations ---")

    from keystonedb.builders import BatchGetRequestBuilder

    request = (
        BatchGetRequestBuilder()
        .add_key(b"task#1")
        .add_key(b"task#2")
        .build()
    )

    response = await client.batch_get(request)

    print(f"Retrieved {len(response.items)} tasks in batch operation")


async def delete_task(client: Client):
    """Delete a task."""
    print("\n--- Deleting Task ---")

    from keystonedb.builders import DeleteRequestBuilder

    # Delete task#3
    request = DeleteRequestBuilder().partition_key(b"task#3").build()
    await client.delete(request)

    # Also delete from project partition
    request_project = (
        DeleteRequestBuilder()
        .partition_key(b"project#frontend")
        .sort_key(b"task#3")
        .build()
    )
    await client.delete(request_project)

    print("✅ Deleted task#3")


def print_item(item):
    """Pretty-print an item's attributes."""
    for key, value in item.attributes.items():
        # Extract value based on type
        val_str = None
        if value.HasField("S"):
            val_str = value.S
        elif value.HasField("N"):
            val_str = value.N
        elif value.HasField("Bool"):
            val_str = str(value.Bool)
        else:
            val_str = str(value)

        print(f"  {key}: {val_str}")


async def main():
    """Run all examples."""
    print(f"Connecting to KeystoneDB server at {SERVER_ADDR}...")

    async with Client(SERVER_ADDR) as client:
        print("✅ Connected successfully!\n")

        await create_tasks(client)
        await get_task(client)
        await query_tasks(client)
        await batch_operations(client)
        await delete_task(client)

        print("\n✅ All operations completed successfully!")


if __name__ == "__main__":
    import asyncio

    asyncio.run(main())
