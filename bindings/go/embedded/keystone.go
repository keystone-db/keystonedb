package keystone

/*
#cgo CFLAGS: -I${SRCDIR}/../../../c-ffi/include
#cgo LDFLAGS: -L${SRCDIR}/../../../target/release -lkstone_ffi -ldl -lm
#include <keystone.h>
#include <stdlib.h>
*/
import "C"
import (
	"errors"
	"fmt"
	"unsafe"
)

// Database represents a KeystoneDB database instance
type Database struct {
	handle *C.ks_database_t
}

// Item represents a database item
type Item struct {
	handle *C.ks_item_t
}

// Error codes
var (
	ErrNullPointer            = errors.New("null pointer")
	ErrInvalidUtf8            = errors.New("invalid UTF-8")
	ErrInvalidArgument        = errors.New("invalid argument")
	ErrIo                     = errors.New("I/O error")
	ErrNotFound               = errors.New("not found")
	ErrInternal               = errors.New("internal error")
	ErrCorruption             = errors.New("corruption detected")
	ErrConditionalCheckFailed = errors.New("conditional check failed")
)

// convertError converts C error code to Go error
func convertError(code C.ks_error_t) error {
	switch code {
	case C.Ok:
		return nil
	case C.NullPointer:
		return ErrNullPointer
	case C.InvalidUtf8:
		return ErrInvalidUtf8
	case C.InvalidArgument:
		return ErrInvalidArgument
	case C.IoError:
		return ErrIo
	case C.NotFound:
		return ErrNotFound
	case C.Internal:
		// Get detailed error message
		msg := C.ks_get_last_error()
		if msg != nil {
			return fmt.Errorf("%w: %s", ErrInternal, C.GoString(msg))
		}
		return ErrInternal
	case C.Corruption:
		return ErrCorruption
	case C.ConditionalCheckFailed:
		return ErrConditionalCheckFailed
	default:
		return fmt.Errorf("unknown error code: %d", code)
	}
}

// Create creates a new database at the specified path
func Create(path string) (*Database, error) {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))

	var handle *C.ks_database_t
	code := C.ks_database_create(cPath, &handle)
	if err := convertError(C.ks_error_t(code)); err != nil {
		return nil, err
	}

	return &Database{handle: handle}, nil
}

// Open opens an existing database at the specified path
func Open(path string) (*Database, error) {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))

	var handle *C.ks_database_t
	code := C.ks_database_open(cPath, &handle)
	if err := convertError(C.ks_error_t(code)); err != nil {
		return nil, err
	}

	return &Database{handle: handle}, nil
}

// CreateInMemory creates a new in-memory database
func CreateInMemory() (*Database, error) {
	var handle *C.ks_database_t
	code := C.ks_database_create_in_memory(&handle)
	if err := convertError(C.ks_error_t(code)); err != nil {
		return nil, err
	}

	return &Database{handle: handle}, nil
}

// Close closes the database
func (db *Database) Close() error {
	if db.handle != nil {
		C.ks_database_close(db.handle)
		db.handle = nil
	}
	return nil
}

// PutString stores a string value in the database
func (db *Database) PutString(pk, sk, attrName, value string) error {
	cPk := C.CString(pk)
	defer C.free(unsafe.Pointer(cPk))

	cAttrName := C.CString(attrName)
	defer C.free(unsafe.Pointer(cAttrName))

	cValue := C.CString(value)
	defer C.free(unsafe.Pointer(cValue))

	var cSk *C.char
	if sk != "" {
		cSk = C.CString(sk)
		defer C.free(unsafe.Pointer(cSk))
	}

	code := C.ks_database_put_string(db.handle, cPk, cSk, cAttrName, cValue)
	return convertError(C.ks_error_t(code))
}

// Put stores an item with only a partition key
func (db *Database) Put(pk, attrName, value string) error {
	return db.PutString(pk, "", attrName, value)
}

// PutWithSK stores an item with partition key and sort key
func (db *Database) PutWithSK(pk, sk, attrName, value string) error {
	return db.PutString(pk, sk, attrName, value)
}

// Get retrieves an item by partition key
func (db *Database) Get(pk string) (*Item, error) {
	cPk := C.CString(pk)
	defer C.free(unsafe.Pointer(cPk))

	var itemHandle *C.ks_item_t
	code := C.ks_database_get(db.handle, cPk, nil, &itemHandle)
	if err := convertError(C.ks_error_t(code)); err != nil {
		return nil, err
	}

	if itemHandle == nil {
		return nil, ErrNotFound
	}

	return &Item{handle: itemHandle}, nil
}

// GetWithSK retrieves an item by partition key and sort key
func (db *Database) GetWithSK(pk, sk string) (*Item, error) {
	cPk := C.CString(pk)
	defer C.free(unsafe.Pointer(cPk))

	cSk := C.CString(sk)
	defer C.free(unsafe.Pointer(cSk))

	var itemHandle *C.ks_item_t
	code := C.ks_database_get(db.handle, cPk, cSk, &itemHandle)
	if err := convertError(C.ks_error_t(code)); err != nil {
		return nil, err
	}

	if itemHandle == nil {
		return nil, ErrNotFound
	}

	return &Item{handle: itemHandle}, nil
}

// Delete removes an item by partition key
func (db *Database) Delete(pk string) error {
	cPk := C.CString(pk)
	defer C.free(unsafe.Pointer(cPk))

	code := C.ks_database_delete(db.handle, cPk, nil)
	return convertError(C.ks_error_t(code))
}

// DeleteWithSK removes an item by partition key and sort key
func (db *Database) DeleteWithSK(pk, sk string) error {
	cPk := C.CString(pk)
	defer C.free(unsafe.Pointer(cPk))

	cSk := C.CString(sk)
	defer C.free(unsafe.Pointer(cSk))

	code := C.ks_database_delete(db.handle, cPk, cSk)
	return convertError(C.ks_error_t(code))
}

// Free frees the item handle
func (item *Item) Free() {
	if item.handle != nil {
		C.ks_item_free(item.handle)
		item.handle = nil
	}
}
