package client

import pb "github.com/keystone-db/keystonedb/bindings/go/client/pb"

// PutRequestBuilder helps build PutRequest
type PutRequestBuilder struct {
	req *pb.PutRequest
}

// NewPutRequest creates a new PutRequestBuilder
func NewPutRequest(pk []byte) *PutRequestBuilder {
	return &PutRequestBuilder{
		req: &pb.PutRequest{
			PartitionKey:      pk,
			Item:              &pb.Item{Attributes: make(map[string]*pb.Value)},
			ExpressionValues:  make(map[string]*pb.Value),
		},
	}
}

// WithSortKey sets the sort key
func (b *PutRequestBuilder) WithSortKey(sk []byte) *PutRequestBuilder {
	b.req.SortKey = &sk
	return b
}

// WithAttribute adds an attribute to the item
func (b *PutRequestBuilder) WithAttribute(name string, value *pb.Value) *PutRequestBuilder {
	b.req.Item.Attributes[name] = value
	return b
}

// WithString adds a string attribute
func (b *PutRequestBuilder) WithString(name, value string) *PutRequestBuilder {
	b.req.Item.Attributes[name] = &pb.Value{
		Value: &pb.Value_StringValue{StringValue: value},
	}
	return b
}

// WithNumber adds a number attribute
func (b *PutRequestBuilder) WithNumber(name, value string) *PutRequestBuilder {
	b.req.Item.Attributes[name] = &pb.Value{
		Value: &pb.Value_NumberValue{NumberValue: value},
	}
	return b
}

// WithBool adds a boolean attribute
func (b *PutRequestBuilder) WithBool(name string, value bool) *PutRequestBuilder {
	b.req.Item.Attributes[name] = &pb.Value{
		Value: &pb.Value_BoolValue{BoolValue: value},
	}
	return b
}

// WithCondition sets a condition expression
func (b *PutRequestBuilder) WithCondition(condition string) *PutRequestBuilder {
	b.req.ConditionExpression = &condition
	return b
}

// Build returns the built PutRequest
func (b *PutRequestBuilder) Build() *pb.PutRequest {
	return b.req
}

// GetRequestBuilder helps build GetRequest
type GetRequestBuilder struct {
	req *pb.GetRequest
}

// NewGetRequest creates a new GetRequestBuilder
func NewGetRequest(pk []byte) *GetRequestBuilder {
	return &GetRequestBuilder{
		req: &pb.GetRequest{
			PartitionKey: pk,
		},
	}
}

// WithSortKey sets the sort key
func (b *GetRequestBuilder) WithSortKey(sk []byte) *GetRequestBuilder {
	b.req.SortKey = &sk
	return b
}

// Build returns the built GetRequest
func (b *GetRequestBuilder) Build() *pb.GetRequest {
	return b.req
}

// QueryRequestBuilder helps build QueryRequest
type QueryRequestBuilder struct {
	req *pb.QueryRequest
}

// NewQueryRequest creates a new QueryRequestBuilder
func NewQueryRequest(pk []byte) *QueryRequestBuilder {
	return &QueryRequestBuilder{
		req: &pb.QueryRequest{
			PartitionKey:     pk,
			ExpressionValues: make(map[string]*pb.Value),
		},
	}
}

// WithSortKeyEqual sets an equal condition on the sort key
func (b *QueryRequestBuilder) WithSortKeyEqual(value *pb.Value) *QueryRequestBuilder {
	b.req.SortKeyCondition = &pb.SortKeyCondition{
		Condition: &pb.SortKeyCondition_EqualTo{EqualTo: value},
	}
	return b
}

// WithSortKeyBeginsWith sets a begins_with condition on the sort key
func (b *QueryRequestBuilder) WithSortKeyBeginsWith(value *pb.Value) *QueryRequestBuilder {
	b.req.SortKeyCondition = &pb.SortKeyCondition{
		Condition: &pb.SortKeyCondition_BeginsWith{BeginsWith: value},
	}
	return b
}

// WithSortKeyBetween sets a between condition on the sort key
func (b *QueryRequestBuilder) WithSortKeyBetween(lower, upper *pb.Value) *QueryRequestBuilder {
	b.req.SortKeyCondition = &pb.SortKeyCondition{
		Condition: &pb.SortKeyCondition_Between{
			Between: &pb.BetweenCondition{Lower: lower, Upper: upper},
		},
	}
	return b
}

// WithLimit sets the query limit
func (b *QueryRequestBuilder) WithLimit(limit uint32) *QueryRequestBuilder {
	b.req.Limit = &limit
	return b
}

// WithIndex sets the index name
func (b *QueryRequestBuilder) WithIndex(indexName string) *QueryRequestBuilder {
	b.req.IndexName = &indexName
	return b
}

// WithScanForward sets the scan direction
func (b *QueryRequestBuilder) WithScanForward(forward bool) *QueryRequestBuilder {
	b.req.ScanForward = &forward
	return b
}

// Build returns the built QueryRequest
func (b *QueryRequestBuilder) Build() *pb.QueryRequest {
	return b.req
}

// ScanRequestBuilder helps build ScanRequest
type ScanRequestBuilder struct {
	req *pb.ScanRequest
}

// NewScanRequest creates a new ScanRequestBuilder
func NewScanRequest() *ScanRequestBuilder {
	return &ScanRequestBuilder{
		req: &pb.ScanRequest{
			ExpressionValues: make(map[string]*pb.Value),
		},
	}
}

// WithLimit sets the scan limit
func (b *ScanRequestBuilder) WithLimit(limit uint32) *ScanRequestBuilder {
	b.req.Limit = &limit
	return b
}

// WithSegment sets the scan segment
func (b *ScanRequestBuilder) WithSegment(segment, totalSegments uint32) *ScanRequestBuilder {
	b.req.Segment = &segment
	b.req.TotalSegments = &totalSegments
	return b
}

// Build returns the built ScanRequest
func (b *ScanRequestBuilder) Build() *pb.ScanRequest {
	return b.req
}
