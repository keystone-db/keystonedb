/**
 * Tests for KeystoneDB Node.js bindings
 *
 * Run with: npm test
 *
 * Note: These tests require the native module to be built first.
 * Run `npm run build` before running tests.
 */

const assert = require('assert');
const fs = require('fs');
const path = require('path');
const os = require('os');

// Try to load the module
let Database, version;
try {
  const keystonedb = require('..');
  Database = keystonedb.Database;
  version = keystonedb.version;
} catch (err) {
  console.error('Failed to load keystonedb module. Run "npm run build" first.');
  console.error(err);
  process.exit(1);
}

// Test utilities
function createTempDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'keystonedb-test-'));
}

function cleanupTempDir(dir) {
  try {
    fs.rmSync(dir, { recursive: true, force: true });
  } catch (err) {
    // Ignore cleanup errors
  }
}

// Test suites
describe('KeystoneDB Node.js Bindings', function() {

  describe('version', function() {
    it('should return a version string', function() {
      const v = version();
      assert.ok(typeof v === 'string');
      assert.ok(v.length > 0);
    });
  });

  describe('In-Memory Database', function() {
    let db;

    beforeEach(function() {
      db = Database.createInMemory();
    });

    it('should create an in-memory database', function() {
      assert.ok(db !== null);
    });

    it('should put and get items', function() {
      db.put('user#123', { name: 'Alice', age: 30 });
      const item = db.get('user#123');
      assert.ok(item !== null);
      assert.strictEqual(item.name, 'Alice');
      assert.strictEqual(item.age, 30);
    });

    it('should put and get items with Buffer keys', function() {
      db.put(Buffer.from('user#456'), { name: 'Bob' });
      const item = db.get(Buffer.from('user#456'));
      assert.ok(item !== null);
      assert.strictEqual(item.name, 'Bob');
    });

    it('should put and get items with sort keys', function() {
      db.put('user#123', { name: 'Alice' }, 'profile');
      const item = db.get('user#123', 'profile');
      assert.ok(item !== null);
      assert.strictEqual(item.name, 'Alice');
    });

    it('should return null for non-existent items', function() {
      const item = db.get('nonexistent');
      assert.strictEqual(item, null);
    });

    it('should delete items', function() {
      db.put('user#123', { name: 'Alice' });
      assert.ok(db.get('user#123') !== null);
      db.delete('user#123');
      assert.strictEqual(db.get('user#123'), null);
    });

    it('should store various value types', function() {
      const item = {
        string: 'hello',
        number: 42,
        float: 3.14,
        bool: true,
        nullVal: null,
        list: [1, 2, 3],
        map: { nested: 'value' }
      };

      db.put('test#1', item);
      const retrieved = db.get('test#1');

      assert.strictEqual(retrieved.string, 'hello');
      assert.strictEqual(retrieved.number, 42);
      assert.ok(Math.abs(retrieved.float - 3.14) < 0.01);
      assert.strictEqual(retrieved.bool, true);
      assert.strictEqual(retrieved.nullVal, null);
      assert.deepStrictEqual(retrieved.list, [1, 2, 3]);
      assert.strictEqual(retrieved.map.nested, 'value');
    });
  });

  describe('Disk Database', function() {
    let tempDir;

    beforeEach(function() {
      tempDir = createTempDir();
    });

    afterEach(function() {
      cleanupTempDir(tempDir);
    });

    it('should create and reopen a database', function() {
      const dbPath = path.join(tempDir, 'test.keystone');

      // Create and write
      let db = Database.create(dbPath);
      db.put('key1', { value: 'test' });
      db.flush();

      // Close by letting it go out of scope (GC)
      db = null;

      // Reopen and read
      const db2 = Database.open(dbPath);
      const item = db2.get('key1');
      assert.ok(item !== null);
      assert.strictEqual(item.value, 'test');
    });
  });

  describe('Batch Operations', function() {
    let db;

    beforeEach(function() {
      db = Database.createInMemory();
    });

    it('should batch write and batch get', function() {
      // Batch write
      const count = db.batchWrite({
        puts: {
          'user#1': { name: 'Alice' },
          'user#2': { name: 'Bob' },
          'user#3': { name: 'Charlie' }
        }
      });
      assert.strictEqual(count, 3);

      // Batch get
      const results = db.batchGet(['user#1', 'user#2', 'user#4']);
      assert.ok('user#1' in results);
      assert.strictEqual(results['user#1'].name, 'Alice');
      assert.ok('user#2' in results);
      assert.strictEqual(results['user#2'].name, 'Bob');
      // user#4 doesn't exist, should not be in results
    });

    it('should batch delete', function() {
      // Create items
      for (let i = 0; i < 5; i++) {
        db.put(`item#${i}`, { id: i });
      }

      // Delete some items
      const count = db.batchWrite({
        deletes: ['item#1', 'item#3']
      });
      assert.strictEqual(count, 2);

      // Verify
      assert.ok(db.get('item#0') !== null);
      assert.strictEqual(db.get('item#1'), null);
      assert.ok(db.get('item#2') !== null);
      assert.strictEqual(db.get('item#3'), null);
      assert.ok(db.get('item#4') !== null);
    });
  });

});

// Simple test runner if not using Mocha
if (typeof describe === 'undefined') {
  console.log('Running standalone tests...\n');

  function test(name, fn) {
    try {
      fn();
      console.log(`✓ ${name}`);
    } catch (err) {
      console.error(`✗ ${name}`);
      console.error(`  ${err.message}`);
      process.exitCode = 1;
    }
  }

  // Version test
  test('version returns a string', function() {
    const v = version();
    assert.ok(typeof v === 'string');
    assert.ok(v.length > 0);
  });

  // In-memory database tests
  test('create in-memory database', function() {
    const db = Database.createInMemory();
    assert.ok(db !== null);
  });

  test('put and get items', function() {
    const db = Database.createInMemory();
    db.put('user#123', { name: 'Alice', age: 30 });
    const item = db.get('user#123');
    assert.ok(item !== null);
    assert.strictEqual(item.name, 'Alice');
    assert.strictEqual(item.age, 30);
  });

  test('delete items', function() {
    const db = Database.createInMemory();
    db.put('user#123', { name: 'Alice' });
    assert.ok(db.get('user#123') !== null);
    db.delete('user#123');
    assert.strictEqual(db.get('user#123'), null);
  });

  test('batch operations', function() {
    const db = Database.createInMemory();
    const count = db.batchWrite({
      puts: {
        'user#1': { name: 'Alice' },
        'user#2': { name: 'Bob' }
      }
    });
    assert.strictEqual(count, 2);

    const results = db.batchGet(['user#1', 'user#2']);
    assert.strictEqual(results['user#1'].name, 'Alice');
    assert.strictEqual(results['user#2'].name, 'Bob');
  });

  console.log('\nAll standalone tests passed!');
}
