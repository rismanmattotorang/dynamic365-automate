//! Transactional write orchestration via OData `$batch` change sets.
//!
//! Where a SAP write BAPI must be followed by `BAPI_TRANSACTION_COMMIT`,
//! Dynamics 365 stages mutating operations into an OData **`$batch` change
//! set** and submits it as a single atomic unit of work — the server applies
//! all operations or none. There is no auto-commit and no partial apply.
//!
//! This module wraps that protocol so every write path enforces it identically:
//!
//! 1. refuse outright in read-only mode (fail-closed);
//! 2. submit the operation inside a change set;
//! 3. inspect the returned Infolog — any `Error`/unknown severity means the
//!    change set was rejected (nothing persisted);
//! 4. fail-closed: if there is no positive confirmation (empty Infolog), treat
//!    the outcome as unconfirmed and report it as not committed.
//!
//! `has_failure` is a pure function so it can be tested directly; the
//! orchestration is generic over [`D365Client`], so it works against the mock
//! and a future live OData backend alike.

use crate::client::{D365Client, ServiceCallRequest};
use crate::error::{D365Error, D365Result};
use crate::infolog::{parse_infolog, InfologMessage, InfologSeverity};
use serde::Serialize;
use serde_json::Value;

/// Control operation name reserved for the batch envelope itself.
const BATCH_OP: &str = "$batch";

fn note(severity: InfologSeverity, text: &str) -> InfologMessage {
    InfologMessage {
        severity,
        code: "D365AUTO".into(),
        text: text.into(),
        field: None,
        environment: None,
    }
}

/// Result of a transactional write.
#[derive(Debug, Clone, Serialize)]
pub struct WriteOutcome {
    pub operation: String,
    /// Whether the change set committed — i.e. the change is persisted.
    pub committed: bool,
    /// Whether the change set was rejected / rolled back (nothing persisted).
    pub rolled_back: bool,
    /// Infolog messages returned by the operation.
    pub messages: Vec<InfologMessage>,
    /// The raw operation result (outputs), for the caller to mine for the
    /// resulting document number etc.
    pub result: Value,
}

/// True if any message indicates the operation failed (so the change set must
/// NOT be considered committed). Unknown severities count as failures.
pub fn has_failure(messages: &[InfologMessage]) -> bool {
    messages.iter().any(InfologMessage::is_failure)
}

/// Execute a write operation inside an atomic `$batch` change set.
///
/// `read_only_mode` mirrors the server's safety flag; when true this refuses
/// to run at all. The underlying `call_service` still applies the client's own
/// read-only gate, so this is defence in depth.
pub async fn execute_write_operation(
    client: &dyn D365Client,
    request: ServiceCallRequest,
    read_only_mode: bool,
) -> D365Result<WriteOutcome> {
    if read_only_mode {
        return Err(D365Error::PermissionDenied(format!(
            "write workflow for '{}' requires write mode (--enable-writes)",
            request.operation
        )));
    }
    if request.operation.eq_ignore_ascii_case(BATCH_OP) {
        return Err(D365Error::InvalidParameter {
            name: "operation".into(),
            reason: "the $batch envelope is constructed automatically; call the business operation instead"
                .into(),
        });
    }

    let operation = request.operation.clone();
    // Submit the operation inside the change set. A live backend wraps this in
    // a multipart `$batch` POST; the atomic semantics are the same.
    let result = client.call_service(request, false).await?;
    let messages = parse_infolog(&result);

    if has_failure(&messages) {
        // The server rejected the change set; nothing persisted.
        return Ok(WriteOutcome {
            operation,
            committed: false,
            rolled_back: true,
            messages,
            result,
        });
    }

    // FAIL-CLOSED: no positive confirmation (empty Infolog) → do not report a
    // commit on faith. The atomic change set is treated as unconfirmed.
    if messages.is_empty() {
        let out = vec![note(
            InfologSeverity::Warning,
            "operation returned no Infolog; outcome unconfirmed — not committed",
        )];
        return Ok(WriteOutcome {
            operation,
            committed: false,
            rolled_back: true,
            messages: out,
            result,
        });
    }

    // Non-empty and no failure → the change set committed atomically.
    Ok(WriteOutcome {
        operation,
        committed: true,
        rolled_back: false,
        messages,
        result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{
        BulkMetadata, EntityRow, EntityStructure, EnvironmentInfo, MockD365Client,
        ReadEntityRequest, ServiceOperationMeta, ServiceSearchResult,
    };
    use async_trait::async_trait;
    use serde_json::json;

    fn msg(sev: InfologSeverity) -> InfologMessage {
        InfologMessage {
            severity: sev,
            code: "X".into(),
            text: "t".into(),
            field: None,
            environment: None,
        }
    }

    #[test]
    fn has_failure_treats_error_and_unknown_as_failure() {
        assert!(has_failure(&[msg(InfologSeverity::Error)]));
        assert!(has_failure(&[msg(InfologSeverity::Unknown('Z'))]));
        assert!(!has_failure(&[msg(InfologSeverity::Success)]));
        assert!(!has_failure(&[
            msg(InfologSeverity::Warning),
            msg(InfologSeverity::Info)
        ]));
        assert!(!has_failure(&[]));
    }

    #[tokio::test]
    async fn refuses_in_read_only_mode() {
        let client = MockD365Client::new(2, json!({ "legal_entity": "USMF" }));
        let req = ServiceCallRequest {
            operation: "PurchaseOrderCreate".into(),
            parameters: json!({ "OrderAccount": "V-001", "dataAreaId": "USMF", "PurchaseOrderLines": [] }),
            timeout_ms: 1000,
            require_read_only_safe: true,
        };
        let err = execute_write_operation(client.as_ref(), req, true)
            .await
            .unwrap_err();
        assert!(matches!(err, D365Error::PermissionDenied(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn rejects_direct_batch_call() {
        let client = MockD365Client::new(2, json!({ "legal_entity": "USMF" }));
        let req = ServiceCallRequest {
            operation: "$batch".into(),
            parameters: Value::Null,
            timeout_ms: 1000,
            require_read_only_safe: false,
        };
        let err = execute_write_operation(client.as_ref(), req, false)
            .await
            .unwrap_err();
        assert!(
            matches!(err, D365Error::InvalidParameter { .. }),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn empty_infolog_is_fail_closed_not_committed() {
        // The mock returns no Infolog, which is an *unconfirmed* outcome —
        // the gate must NOT report it as committed.
        let client = MockD365Client::new(2, json!({ "legal_entity": "USMF" }));
        let req = ServiceCallRequest {
            operation: "PurchaseOrderCreate".into(),
            parameters: json!({ "OrderAccount": "V-001", "dataAreaId": "USMF", "PurchaseOrderLines": [] }),
            timeout_ms: 1000,
            require_read_only_safe: true,
        };
        let outcome = execute_write_operation(client.as_ref(), req, false)
            .await
            .unwrap();
        assert!(!outcome.committed, "empty Infolog must not commit");
        assert!(outcome.rolled_back);
        assert!(outcome
            .messages
            .iter()
            .any(|m| m.text.contains("unconfirmed")));
    }

    // A scripted client returning a canned Infolog for the business operation.
    struct ScriptedClient {
        infolog: Value,
    }

    #[async_trait]
    impl D365Client for ScriptedClient {
        async fn call_service(&self, _request: ServiceCallRequest, _ro: bool) -> D365Result<Value> {
            Ok(json!({ "outputs": { "Infolog": self.infolog.clone() } }))
        }
        async fn environment_info(&self) -> D365Result<EnvironmentInfo> {
            Err(D365Error::Internal("unused".into()))
        }
        async fn search_service(&self, _q: &str, _n: usize) -> D365Result<ServiceSearchResult> {
            Err(D365Error::Internal("unused".into()))
        }
        async fn service_metadata(&self, _o: &str, _l: &str) -> D365Result<ServiceOperationMeta> {
            Err(D365Error::Internal("unused".into()))
        }
        async fn bulk_service_metadata(&self, _o: &[String], _l: &str) -> D365Result<BulkMetadata> {
            Err(D365Error::Internal("unused".into()))
        }
        async fn read_entity(&self, _r: ReadEntityRequest) -> D365Result<Vec<EntityRow>> {
            Err(D365Error::Internal("unused".into()))
        }
        async fn entity_structure(&self, _e: &str) -> D365Result<EntityStructure> {
            Err(D365Error::Internal("unused".into()))
        }
    }

    fn scripted(infolog: Value) -> ScriptedClient {
        ScriptedClient { infolog }
    }

    #[tokio::test]
    async fn commits_on_explicit_success_row() {
        let client = scripted(
            json!([{ "severity": "Info", "code": "PurchPosted", "message": "Purchase order PO-000178 created" }]),
        );
        let req = ServiceCallRequest {
            operation: "PurchaseOrderCreate".into(),
            parameters: json!({ "OrderAccount": "V-001" }),
            timeout_ms: 1000,
            require_read_only_safe: true,
        };
        let outcome = execute_write_operation(&client, req, false).await.unwrap();
        assert!(
            outcome.committed,
            "expected commit; messages={:?}",
            outcome.messages
        );
        assert!(!outcome.rolled_back);
    }

    #[tokio::test]
    async fn rolls_back_on_error_row() {
        let client = scripted(
            json!([{ "severity": "Error", "code": "VendBlocked", "message": "Vendor V-001 is on hold" }]),
        );
        let req = ServiceCallRequest {
            operation: "PurchaseOrderCreate".into(),
            parameters: json!({ "OrderAccount": "V-001" }),
            timeout_ms: 1000,
            require_read_only_safe: true,
        };
        let outcome = execute_write_operation(&client, req, false).await.unwrap();
        assert!(!outcome.committed, "error row must not commit");
        assert!(
            outcome.rolled_back,
            "error row must roll back the change set"
        );
    }
}
