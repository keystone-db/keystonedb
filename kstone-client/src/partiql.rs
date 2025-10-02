/// Remote PartiQL operations
use crate::convert::*;
use crate::error::{ClientError, Result};
use bytes::Bytes;
use kstone_core::Item;
use kstone_proto as proto;

/// Execute statement response (mirrors kstone-api ExecuteStatementResponse)
#[derive(Debug)]
pub enum RemoteExecuteStatementResponse {
    /// SELECT statement result
    Select {
        items: Vec<Item>,
        count: usize,
        scanned_count: usize,
        last_key: Option<(Bytes, Option<Bytes>)>,
    },
    /// INSERT statement result
    Insert { success: bool },
    /// UPDATE statement result
    Update { item: Item },
    /// DELETE statement result
    Delete { success: bool },
}

/// Parse ExecuteStatementResponse from protobuf
pub(crate) fn parse_execute_statement_response(
    response: proto::ExecuteStatementResponse,
) -> Result<RemoteExecuteStatementResponse> {
    use proto::execute_statement_response::Response as ProtoResponse;

    let proto_response = response
        .response
        .ok_or_else(|| ClientError::InternalError("Empty response from server".to_string()))?;

    match proto_response {
        ProtoResponse::Select(select_result) => {
            let items: Vec<Item> = select_result
                .items
                .into_iter()
                .map(|proto_item| proto_item_to_ks(proto_item).expect("Invalid item from server"))
                .collect();

            let last_key = select_result
                .last_key
                .map(proto_last_key_to_ks);

            Ok(RemoteExecuteStatementResponse::Select {
                items,
                count: select_result.count as usize,
                scanned_count: select_result.scanned_count as usize,
                last_key,
            })
        }
        ProtoResponse::Insert(insert_result) => {
            Ok(RemoteExecuteStatementResponse::Insert {
                success: insert_result.success,
            })
        }
        ProtoResponse::Update(update_result) => {
            let item = proto_item_to_ks(
                update_result.item.expect("Server should return updated item")
            )?;

            Ok(RemoteExecuteStatementResponse::Update { item })
        }
        ProtoResponse::Delete(delete_result) => {
            Ok(RemoteExecuteStatementResponse::Delete {
                success: delete_result.success,
            })
        }
    }
}
