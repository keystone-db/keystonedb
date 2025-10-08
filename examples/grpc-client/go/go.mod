module github.com/keystone-db/keystonedb/examples/grpc-client/go

go 1.21

replace github.com/keystone-db/keystonedb/bindings/go/client => ../../../bindings/go/client

require (
	github.com/keystone-db/keystonedb/bindings/go/client v0.0.0-00010101000000-000000000000
	google.golang.org/grpc v1.65.0
)

require (
	golang.org/x/net v0.26.0 // indirect
	golang.org/x/sys v0.21.0 // indirect
	golang.org/x/text v0.16.0 // indirect
	google.golang.org/genproto/googleapis/rpc v0.0.0-20240604185151-ef581f913117 // indirect
	google.golang.org/protobuf v1.34.2 // indirect
)
