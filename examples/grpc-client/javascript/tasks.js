#!/usr/bin/env node

/**
 * KeystoneDB gRPC JavaScript Client Example: Task Management
 *
 * Demonstrates remote database operations using the JavaScript gRPC client.
 */

const path = require('path');

// Import the KeystoneDB client
const clientPath = path.join(__dirname, '../../../bindings/javascript/client/dist');
let KeystoneClient;

try {
    const module = require(clientPath);
    KeystoneClient = module.KeystoneClient || module.default?.KeystoneClient;

    if (!KeystoneClient) {
        console.error('Error: KeystoneClient not found in module');
        console.error('Available exports:', Object.keys(module));
        process.exit(1);
    }
} catch (error) {
    console.error('Error importing KeystoneDB client:', error.message);
    console.error('\nMake sure you\'ve built the JavaScript client:');
    console.error('  cd bindings/javascript/client');
    console.error('  npm install');
    console.error('  npm run build');
    process.exit(1);
}

const SERVER_ADDR = 'localhost:50051';

async function createTasks(client) {
    console.log('--- Creating Tasks ---');

    const tasks = [
        {
            id: 'task#1',
            project: 'project#backend',
            title: 'Implement user authentication',
            description: 'Add JWT-based auth system',
            status: 'in-progress',
            priority: 'high',
        },
        {
            id: 'task#2',
            project: 'project#backend',
            title: 'Set up database migrations',
            description: 'Create migration scripts',
            status: 'pending',
            priority: 'medium',
        },
        {
            id: 'task#3',
            project: 'project#frontend',
            title: 'Design login page',
            description: 'Create UI mockups',
            status: 'completed',
            priority: 'low',
        },
    ];

    for (const task of tasks) {
        // Build item attributes
        const attributes = {
            title: { S: task.title },
            description: { S: task.description },
            status: { S: task.status },
            priority: { S: task.priority },
            created: { N: String(Math.floor(Date.now() / 1000)) },
        };

        // Put task by ID
        await client.put({
            partitionKey: Buffer.from(task.id),
            item: { attributes },
        });

        // Also put with project partition for querying
        await client.put({
            partitionKey: Buffer.from(task.project),
            sortKey: Buffer.from(task.id),
            item: { attributes },
        });

        console.log(`✅ Created task: ${task.id}`);
    }
}

async function getTask(client) {
    console.log('\n--- Retrieving Task ---');

    const response = await client.get({
        partitionKey: Buffer.from('task#1'),
    });

    if (response.item) {
        console.log('Task task#1:');
        printItem(response.item);
    } else {
        console.log('Task not found');
    }
}

async function queryTasks(client) {
    console.log('\n--- Querying Tasks by Project ---');

    const response = await client.query({
        partitionKey: Buffer.from('project#backend'),
        limit: 10,
    });

    console.log(`Found ${response.items.length} tasks for project#backend`);

    response.items.forEach((item, i) => {
        console.log(`\nTask ${i + 1}:`);
        printItem(item);
    });
}

async function batchOperations(client) {
    console.log('\n--- Batch Operations ---');

    const response = await client.batchGet({
        keys: [
            { partitionKey: Buffer.from('task#1') },
            { partitionKey: Buffer.from('task#2') },
        ],
    });

    console.log(`Retrieved ${response.items.length} tasks in batch operation`);
}

async function deleteTask(client) {
    console.log('\n--- Deleting Task ---');

    // Delete task#3
    await client.delete({
        partitionKey: Buffer.from('task#3'),
    });

    // Also delete from project partition
    await client.delete({
        partitionKey: Buffer.from('project#frontend'),
        sortKey: Buffer.from('task#3'),
    });

    console.log('✅ Deleted task#3');
}

function printItem(item) {
    if (!item.attributes) return;

    for (const [key, value] of Object.entries(item.attributes)) {
        let valStr;
        if (value.S !== undefined) {
            valStr = value.S;
        } else if (value.N !== undefined) {
            valStr = value.N;
        } else if (value.Bool !== undefined) {
            valStr = String(value.Bool);
        } else {
            valStr = JSON.stringify(value);
        }
        console.log(`  ${key}: ${valStr}`);
    }
}

async function main() {
    console.log(`Connecting to KeystoneDB server at ${SERVER_ADDR}...`);

    const client = new KeystoneClient(SERVER_ADDR);

    try {
        console.log('✅ Connected successfully!\n');

        await createTasks(client);
        await getTask(client);
        await queryTasks(client);
        await batchOperations(client);
        await deleteTask(client);

        console.log('\n✅ All operations completed successfully!');
    } catch (error) {
        console.error('Error:', error.message);
        process.exit(1);
    } finally {
        client.close();
    }
}

// Run if executed directly
if (require.main === module) {
    main().catch((error) => {
        console.error('Fatal error:', error);
        process.exit(1);
    });
}
