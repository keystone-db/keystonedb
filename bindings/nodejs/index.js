/**
 * KeystoneDB Node.js bindings
 *
 * An embedded DynamoDB-compatible database for Node.js.
 *
 * @example
 * ```javascript
 * const { Database } = require('keystonedb');
 * const db = Database.create('mydb.keystone');
 * db.put('user#123', { name: 'Alice', age: 30 });
 * const item = db.get('user#123');
 * console.log(item.name); // Alice
 * ```
 */

const { platform, arch } = process;

let nativeBinding = null;
let loadError = null;

// Try to load the native binding
const bindingName = `keystonedb.${platform}-${arch}`;

try {
  // First, try the platform-specific binding
  nativeBinding = require(`./${bindingName}.node`);
} catch (e) {
  loadError = e;

  try {
    // Fallback to generic binding
    nativeBinding = require('./keystonedb.node');
  } catch (e2) {
    // Try to load from common build locations
    const path = require('path');
    const possiblePaths = [
      // Debug build
      path.join(__dirname, 'target', 'debug', 'keystonedb.node'),
      // Release build
      path.join(__dirname, 'target', 'release', 'keystonedb.node'),
    ];

    for (const p of possiblePaths) {
      try {
        nativeBinding = require(p);
        break;
      } catch (e3) {
        // Continue trying
      }
    }

    if (!nativeBinding) {
      loadError = new Error(
        `Failed to load native binding for ${platform}-${arch}.\n` +
        `Original error: ${e.message}\n` +
        'Please run "npm run build" to build the native module.'
      );
    }
  }
}

if (!nativeBinding) {
  throw loadError;
}

module.exports = nativeBinding;
