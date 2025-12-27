/**
 * KeystoneDB C Foreign Function Interface
 *
 * This header provides C-compatible bindings for KeystoneDB,
 * enabling integration with C, Python, JavaScript, and other languages.
 *
 * Memory Management:
 * - All objects returned by kstone_*_create functions must be freed with corresponding kstone_*_free functions
 * - Strings returned by functions are owned and must be freed with kstone_string_free
 * - The caller is responsible for freeing all allocated memory
 *
 * Error Handling:
 * - Functions return NULL or error codes on failure
 * - Use kstone_last_error() to get the last error message
 * - Use kstone_last_error_code() to get the last error code
 */

#ifndef KEYSTONEDB_H
#define KEYSTONEDB_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Error codes */
#define KSTONE_OK                    0
#define KSTONE_ERR_NULL_POINTER     -1
#define KSTONE_ERR_INVALID_UTF8     -2
#define KSTONE_ERR_NOT_FOUND        -3
#define KSTONE_ERR_INVALID_QUERY    -4
#define KSTONE_ERR_INVALID_ARGUMENT -5
#define KSTONE_ERR_CONDITION_FAILED -6
#define KSTONE_ERR_TRANSACTION_CANCELED -7
#define KSTONE_ERR_IO               -8
#define KSTONE_ERR_CORRUPTION       -9
#define KSTONE_ERR_INTERNAL        -10

/* Opaque handle types */
typedef struct KstoneDb KstoneDb;
typedef struct KstoneItem KstoneItem;
typedef struct KstoneValue KstoneValue;
typedef struct KstoneQuery KstoneQuery;
typedef struct KstoneScan KstoneScan;
typedef struct KstoneUpdate KstoneUpdate;
typedef struct KstoneBatchGet KstoneBatchGet;
typedef struct KstoneBatchWrite KstoneBatchWrite;
typedef struct KstoneItemBuilder KstoneItemBuilder;

/* Query/Scan response structure */
typedef struct KstoneQueryResponse {
    KstoneItem** items;
    unsigned int item_count;
    unsigned int scanned_count;
    int has_more;
    char* last_pk;
    char* last_sk;
} KstoneQueryResponse;

/* ============================================================================
 * Error Handling
 * ============================================================================ */

/**
 * Get the last error message. Returns NULL if no error.
 * The returned string must be freed with kstone_string_free().
 */
char* kstone_last_error(void);

/**
 * Get the last error code. Returns KSTONE_OK if no error.
 */
int kstone_last_error_code(void);

/**
 * Clear the last error.
 */
void kstone_clear_error(void);

/**
 * Free a string returned by KeystoneDB functions.
 */
void kstone_string_free(char* s);

/* ============================================================================
 * Database Operations
 * ============================================================================ */

/**
 * Create a new database at the given path.
 * @param path Path to the database directory (null-terminated string)
 * @return Database handle, or NULL on error
 */
KstoneDb* kstone_db_create(const char* path);

/**
 * Open an existing database at the given path.
 * @param path Path to the database directory (null-terminated string)
 * @return Database handle, or NULL on error
 */
KstoneDb* kstone_db_open(const char* path);

/**
 * Create an in-memory database (no persistence).
 * @return Database handle, or NULL on error
 */
KstoneDb* kstone_db_create_in_memory(void);

/**
 * Close and free a database handle.
 * @param db Database handle (may be NULL)
 */
void kstone_db_close(KstoneDb* db);

/**
 * Flush pending writes to disk.
 * @param db Database handle
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_db_flush(KstoneDb* db);

/* ============================================================================
 * CRUD Operations
 * ============================================================================ */

/**
 * Put an item into the database.
 * @param db Database handle
 * @param pk Partition key bytes
 * @param pk_len Length of partition key
 * @param item Item to store
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_db_put(KstoneDb* db, const uint8_t* pk, unsigned int pk_len, const KstoneItem* item);

/**
 * Put an item with a sort key into the database.
 * @param db Database handle
 * @param pk Partition key bytes
 * @param pk_len Length of partition key
 * @param sk Sort key bytes
 * @param sk_len Length of sort key
 * @param item Item to store
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_db_put_with_sk(KstoneDb* db, const uint8_t* pk, unsigned int pk_len,
                          const uint8_t* sk, unsigned int sk_len, const KstoneItem* item);

/**
 * Get an item from the database.
 * @param db Database handle
 * @param pk Partition key bytes
 * @param pk_len Length of partition key
 * @return Item handle, or NULL if not found (check kstone_last_error_code())
 */
KstoneItem* kstone_db_get(KstoneDb* db, const uint8_t* pk, unsigned int pk_len);

/**
 * Get an item with a sort key from the database.
 * @param db Database handle
 * @param pk Partition key bytes
 * @param pk_len Length of partition key
 * @param sk Sort key bytes
 * @param sk_len Length of sort key
 * @return Item handle, or NULL if not found
 */
KstoneItem* kstone_db_get_with_sk(KstoneDb* db, const uint8_t* pk, unsigned int pk_len,
                                   const uint8_t* sk, unsigned int sk_len);

/**
 * Delete an item from the database.
 * @param db Database handle
 * @param pk Partition key bytes
 * @param pk_len Length of partition key
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_db_delete(KstoneDb* db, const uint8_t* pk, unsigned int pk_len);

/**
 * Delete an item with a sort key from the database.
 * @param db Database handle
 * @param pk Partition key bytes
 * @param pk_len Length of partition key
 * @param sk Sort key bytes
 * @param sk_len Length of sort key
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_db_delete_with_sk(KstoneDb* db, const uint8_t* pk, unsigned int pk_len,
                              const uint8_t* sk, unsigned int sk_len);

/* ============================================================================
 * Item Operations
 * ============================================================================ */

/**
 * Create a new empty item.
 * @return Item handle
 */
KstoneItem* kstone_item_new(void);

/**
 * Free an item.
 * @param item Item handle (may be NULL)
 */
void kstone_item_free(KstoneItem* item);

/**
 * Set a string attribute on an item.
 * @param item Item handle
 * @param key Attribute name (null-terminated)
 * @param value Attribute value (null-terminated)
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_item_set_string(KstoneItem* item, const char* key, const char* value);

/**
 * Set a number attribute on an item (as integer).
 * @param item Item handle
 * @param key Attribute name (null-terminated)
 * @param value Integer value
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_item_set_number_int(KstoneItem* item, const char* key, int64_t value);

/**
 * Set a number attribute on an item (as double).
 * @param item Item handle
 * @param key Attribute name (null-terminated)
 * @param value Double value
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_item_set_number_double(KstoneItem* item, const char* key, double value);

/**
 * Set a boolean attribute on an item.
 * @param item Item handle
 * @param key Attribute name (null-terminated)
 * @param value Boolean value (0 = false, non-zero = true)
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_item_set_bool(KstoneItem* item, const char* key, int value);

/**
 * Set a null attribute on an item.
 * @param item Item handle
 * @param key Attribute name (null-terminated)
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_item_set_null(KstoneItem* item, const char* key);

/**
 * Set a binary attribute on an item.
 * @param item Item handle
 * @param key Attribute name (null-terminated)
 * @param value Binary data
 * @param value_len Length of binary data
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_item_set_binary(KstoneItem* item, const char* key, const uint8_t* value, unsigned int value_len);

/**
 * Get a string attribute from an item.
 * @param item Item handle
 * @param key Attribute name (null-terminated)
 * @return String value (must be freed with kstone_string_free), or NULL if not found
 */
char* kstone_item_get_string(const KstoneItem* item, const char* key);

/**
 * Get a number attribute from an item as a double.
 * @param item Item handle
 * @param key Attribute name (null-terminated)
 * @param out_value Pointer to store the result
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_item_get_number(const KstoneItem* item, const char* key, double* out_value);

/**
 * Get a boolean attribute from an item.
 * @param item Item handle
 * @param key Attribute name (null-terminated)
 * @param out_value Pointer to store the result (0 or 1)
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_item_get_bool(const KstoneItem* item, const char* key, int* out_value);

/**
 * Check if an attribute exists in an item.
 * @param item Item handle
 * @param key Attribute name (null-terminated)
 * @return 1 if exists, 0 otherwise
 */
int kstone_item_has_attribute(const KstoneItem* item, const char* key);

/**
 * Get the number of attributes in an item.
 * @param item Item handle
 * @return Number of attributes
 */
unsigned int kstone_item_attribute_count(const KstoneItem* item);

/**
 * Convert an item to JSON string.
 * @param item Item handle
 * @return JSON string (must be freed with kstone_string_free), or NULL on error
 */
char* kstone_item_to_json(const KstoneItem* item);

/**
 * Create an item from JSON string.
 * @param json JSON string (null-terminated)
 * @return Item handle, or NULL on error
 */
KstoneItem* kstone_item_from_json(const char* json);

/* ============================================================================
 * ItemBuilder Operations
 * ============================================================================ */

/**
 * Create a new ItemBuilder.
 * @return ItemBuilder handle
 */
KstoneItemBuilder* kstone_item_builder_new(void);

/**
 * Free an ItemBuilder.
 * @param builder ItemBuilder handle (may be NULL)
 */
void kstone_item_builder_free(KstoneItemBuilder* builder);

/**
 * Add a string attribute to the builder.
 * @param builder ItemBuilder handle
 * @param key Attribute name (null-terminated)
 * @param value Attribute value (null-terminated)
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_item_builder_string(KstoneItemBuilder* builder, const char* key, const char* value);

/**
 * Add a number attribute to the builder.
 * @param builder ItemBuilder handle
 * @param key Attribute name (null-terminated)
 * @param value Integer value
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_item_builder_number(KstoneItemBuilder* builder, const char* key, int64_t value);

/**
 * Add a boolean attribute to the builder.
 * @param builder ItemBuilder handle
 * @param key Attribute name (null-terminated)
 * @param value Boolean value (0 = false, non-zero = true)
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_item_builder_bool(KstoneItemBuilder* builder, const char* key, int value);

/**
 * Build the item from the builder. Consumes the builder.
 * @param builder ItemBuilder handle (freed after this call)
 * @return Item handle, or NULL on error
 */
KstoneItem* kstone_item_builder_build(KstoneItemBuilder* builder);

/* ============================================================================
 * Query Operations
 * ============================================================================ */

/**
 * Create a new Query for the given partition key.
 * @param pk Partition key bytes
 * @param pk_len Length of partition key
 * @return Query handle, or NULL on error
 */
KstoneQuery* kstone_query_new(const uint8_t* pk, unsigned int pk_len);

/**
 * Free a Query.
 * @param query Query handle (may be NULL)
 */
void kstone_query_free(KstoneQuery* query);

/**
 * Set sort key equals condition.
 * @param query Query handle
 * @param sk Sort key bytes
 * @param sk_len Length of sort key
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_query_sk_eq(KstoneQuery* query, const uint8_t* sk, unsigned int sk_len);

/**
 * Set sort key begins_with condition.
 * @param query Query handle
 * @param prefix Prefix bytes
 * @param prefix_len Length of prefix
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_query_sk_begins_with(KstoneQuery* query, const uint8_t* prefix, unsigned int prefix_len);

/**
 * Set sort key greater than condition.
 */
int kstone_query_sk_gt(KstoneQuery* query, const uint8_t* sk, unsigned int sk_len);

/**
 * Set sort key greater than or equal condition.
 */
int kstone_query_sk_gte(KstoneQuery* query, const uint8_t* sk, unsigned int sk_len);

/**
 * Set sort key less than condition.
 */
int kstone_query_sk_lt(KstoneQuery* query, const uint8_t* sk, unsigned int sk_len);

/**
 * Set sort key less than or equal condition.
 */
int kstone_query_sk_lte(KstoneQuery* query, const uint8_t* sk, unsigned int sk_len);

/**
 * Set sort key between condition.
 */
int kstone_query_sk_between(KstoneQuery* query, const uint8_t* sk1, unsigned int sk1_len,
                             const uint8_t* sk2, unsigned int sk2_len);

/**
 * Set query limit.
 * @param query Query handle
 * @param limit Maximum number of items to return
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_query_limit(KstoneQuery* query, unsigned int limit);

/**
 * Set query direction.
 * @param query Query handle
 * @param forward 1 for forward, 0 for reverse
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_query_forward(KstoneQuery* query, int forward);

/**
 * Set index name for the query.
 * @param query Query handle
 * @param index_name Index name (null-terminated)
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_query_index(KstoneQuery* query, const char* index_name);

/**
 * Execute a query. The query is consumed.
 * @param db Database handle
 * @param query Query handle (freed after this call)
 * @return Query response, or NULL on error
 */
KstoneQueryResponse* kstone_db_query(KstoneDb* db, KstoneQuery* query);

/**
 * Free a query response.
 * @param response Query response (may be NULL)
 */
void kstone_query_response_free(KstoneQueryResponse* response);

/**
 * Get an item from a query response by index.
 * @param response Query response
 * @param index Item index (0-based)
 * @return Item handle (do NOT free - owned by response), or NULL if out of bounds
 */
const KstoneItem* kstone_query_response_get_item(const KstoneQueryResponse* response, unsigned int index);

/* ============================================================================
 * Scan Operations
 * ============================================================================ */

/**
 * Create a new Scan.
 * @return Scan handle
 */
KstoneScan* kstone_scan_new(void);

/**
 * Free a Scan.
 * @param scan Scan handle (may be NULL)
 */
void kstone_scan_free(KstoneScan* scan);

/**
 * Set scan limit.
 * @param scan Scan handle
 * @param limit Maximum number of items to return
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_scan_limit(KstoneScan* scan, unsigned int limit);

/**
 * Set scan segment for parallel scans.
 * @param scan Scan handle
 * @param segment Segment number (0-based)
 * @param total_segments Total number of segments
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_scan_segment(KstoneScan* scan, unsigned int segment, unsigned int total_segments);

/**
 * Execute a scan. The scan is consumed.
 * @param db Database handle
 * @param scan Scan handle (freed after this call)
 * @return Query response (use kstone_query_response_* functions), or NULL on error
 */
KstoneQueryResponse* kstone_db_scan(KstoneDb* db, KstoneScan* scan);

/* ============================================================================
 * PartiQL / SQL Operations
 * ============================================================================ */

/**
 * Execute a PartiQL/SQL statement.
 * @param db Database handle
 * @param sql SQL statement (null-terminated)
 * @return JSON string with result (must be freed with kstone_string_free), or NULL on error
 */
char* kstone_db_execute_sql(KstoneDb* db, const char* sql);

/* ============================================================================
 * Batch Operations
 * ============================================================================ */

/**
 * Create a new BatchGetRequest.
 * @return BatchGet handle
 */
KstoneBatchGet* kstone_batch_get_new(void);

/**
 * Free a BatchGetRequest.
 * @param batch BatchGet handle (may be NULL)
 */
void kstone_batch_get_free(KstoneBatchGet* batch);

/**
 * Add a key to the batch get request.
 * @param batch BatchGet handle
 * @param pk Partition key bytes
 * @param pk_len Length of partition key
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_batch_get_add_key(KstoneBatchGet* batch, const uint8_t* pk, unsigned int pk_len);

/**
 * Execute a batch get. The request is consumed.
 * @param db Database handle
 * @param batch BatchGet handle (freed after this call)
 * @return JSON string with results (must be freed with kstone_string_free), or NULL on error
 */
char* kstone_db_batch_get(KstoneDb* db, KstoneBatchGet* batch);

/**
 * Create a new BatchWriteRequest.
 * @return BatchWrite handle
 */
KstoneBatchWrite* kstone_batch_write_new(void);

/**
 * Free a BatchWriteRequest.
 * @param batch BatchWrite handle (may be NULL)
 */
void kstone_batch_write_free(KstoneBatchWrite* batch);

/**
 * Add a put operation to the batch write request.
 * @param batch BatchWrite handle
 * @param pk Partition key bytes
 * @param pk_len Length of partition key
 * @param item Item to store
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_batch_write_put(KstoneBatchWrite* batch, const uint8_t* pk, unsigned int pk_len, const KstoneItem* item);

/**
 * Add a delete operation to the batch write request.
 * @param batch BatchWrite handle
 * @param pk Partition key bytes
 * @param pk_len Length of partition key
 * @return KSTONE_OK on success, error code on failure
 */
int kstone_batch_write_delete(KstoneBatchWrite* batch, const uint8_t* pk, unsigned int pk_len);

/**
 * Execute a batch write. The request is consumed.
 * @param db Database handle
 * @param batch BatchWrite handle (freed after this call)
 * @return Number of processed items, or negative error code
 */
int kstone_db_batch_write(KstoneDb* db, KstoneBatchWrite* batch);

/* ============================================================================
 * Version and Info
 * ============================================================================ */

/**
 * Get the version of the KeystoneDB library.
 * @return Version string (statically allocated, do NOT free)
 */
const char* kstone_version(void);

#ifdef __cplusplus
}
#endif

#endif /* KEYSTONEDB_H */
