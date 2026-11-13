use crate::state::AppState;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use engine_storage::AotCompiler;
use serde::Deserialize;
use serde_json::json;
use tracing::{error, info};

#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    pub format: Option<String>,
}

/// Healthcheck endpoint
pub async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "status": "healthy",
            "engine": "WasmPulse",
            "version": "v0.1.0",
            "features": [
                "virtual_memory_pooling_allocator",
                "cryptographic_attestation_merkle_ledger",
                "aws_cloudwatch_emf_telemetry",
                "linear_memory_sfi_sandboxing",
                "sub_millisecond_cold_start"
            ]
        })),
    )
}

/// Metrics and engine status (supports JSON and Prometheus formats)
pub async fn metrics(
    State(state): State<AppState>,
    Query(query): Query<MetricsQuery>,
) -> Response {
    let functions = state.registry.list();
    let total_functions = functions.len();
    let total_invocations: u64 = functions.iter().map(|f| f.invocation_count).sum();

    if query.format.as_deref() == Some("prometheus") {
        let mut prom = String::new();
        prom.push_str("# HELP wasmpulse_active_functions Total number of active deployed functions\n");
        prom.push_str("# TYPE wasmpulse_active_functions gauge\n");
        prom.push_str(&format!("wasmpulse_active_functions {}\n\n", total_functions));

        prom.push_str("# HELP wasmpulse_total_invocations Total function invocations across all tenants\n");
        prom.push_str("# TYPE wasmpulse_total_invocations counter\n");
        prom.push_str(&format!("wasmpulse_total_invocations {}\n\n", total_invocations));

        for f in &functions {
            prom.push_str(&format!(
                "wasmpulse_function_invocations{{tenant=\"{}\",function=\"{}\"}} {}\n",
                f.tenant_id, f.name, f.invocation_count
            ));
            prom.push_str(&format!(
                "wasmpulse_function_size_bytes{{tenant=\"{}\",function=\"{}\"}} {}\n",
                f.tenant_id, f.name, f.bytecode_size_bytes
            ));
        }

        return Response::builder()
            .header("Content-Type", "text/plain; version=0.0.4; charset=utf-8")
            .body(axum::body::Body::from(prom))
            .unwrap();
    }

    (
        StatusCode::OK,
        Json(json!({
            "engine": "WasmPulse",
            "version": "v0.1.0",
            "active_functions": total_functions,
            "total_invocations": total_invocations,
            "latest_audit_hash": state.executor.ledger().latest_hash(),
            "functions": functions,
        })),
    ).into_response()
}

/// List all deployed functions
pub async fn list_functions(State(state): State<AppState>) -> impl IntoResponse {
    let functions = state.registry.list();
    (StatusCode::OK, Json(functions))
}

/// Deploy a new Wasm function: POST /api/v1/functions
pub async fn deploy_function(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("default");

    let function_name = match headers.get("x-function-name").and_then(|h| h.to_str().ok()) {
        Some(name) => name,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Missing required 'X-Function-Name' header" })),
            ));
        }
    };

    if body.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Uploaded .wasm binary payload is empty" })),
        ));
    }

    info!(tenant = tenant_id, function = function_name, bytes = body.len(), "Compiling and registering Wasm function");

    let wasm_bytes = body.to_vec();
    let module = match AotCompiler::compile_to_module(&state.engine, &wasm_bytes) {
        Ok(m) => m,
        Err(err) => {
            error!("Compilation error: {:?}", err);
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Failed to compile Wasm bytecode: {}", err) })),
            ));
        }
    };

    state.registry.register(tenant_id, function_name, wasm_bytes.len(), module);

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "message": "Function deployed successfully",
            "tenant_id": tenant_id,
            "function_name": function_name,
            "bytecode_size_bytes": wasm_bytes.len(),
            "invoke_url": format!("/fn/{}/{}", tenant_id, function_name)
        })),
    ).into_response())
}

/// Delete a function: DELETE /api/v1/functions/:tenant/:function
pub async fn delete_function(
    State(state): State<AppState>,
    Path((tenant, function)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.registry.remove(&tenant, &function) {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({ "message": format!("Function '{}/{}' deleted", tenant, function) })),
        ),
        Err(err) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": err.to_string() })),
        ),
    }
}

/// List recent verifiable execution receipts: GET /api/v1/audit/receipts
pub async fn list_audit_receipts(State(state): State<AppState>) -> impl IntoResponse {
    let receipts = state.executor.ledger().list(50);
    (StatusCode::OK, Json(receipts))
}

/// Verify a single execution receipt: GET /api/v1/audit/verify/:receipt_id
pub async fn verify_audit_receipt(
    State(state): State<AppState>,
    Path(receipt_id): Path<String>,
) -> impl IntoResponse {
    match state.executor.ledger().get_by_id(&receipt_id) {
        Some(receipt) => {
            let is_valid = receipt.verify();
            (
                StatusCode::OK,
                Json(json!({
                    "receipt_id": receipt_id,
                    "is_valid": is_valid,
                    "proof_hash": receipt.proof_hash,
                    "receipt": receipt
                })),
            )
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Receipt '{}' not found in audit ledger", receipt_id) })),
        ),
    }
}

/// Verify the entire Merkle audit chain: GET /api/v1/audit/chain
pub async fn verify_audit_chain(State(state): State<AppState>) -> impl IntoResponse {
    let (is_valid, count) = state.executor.ledger().verify_chain();
    (
        StatusCode::OK,
        Json(json!({
            "chain_valid": is_valid,
            "receipts_verified": count,
            "latest_proof_hash": state.executor.ledger().latest_hash()
        })),
    )
}

/// Execute a deployed function: POST /fn/:tenant/:function
pub async fn invoke_function(
    State(state): State<AppState>,
    Path((tenant, function)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let module = match state.registry.get(&tenant, &function) {
        Some(m) => m,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("Function '{}/{}' not found", tenant, function) })),
            ));
        }
    };

    let input_data = body.to_vec();

    match state.executor.execute(&module, &tenant, &function, input_data, None, None) {
        Ok(result) => {
            // 1. Emit AWS CloudWatch Embedded Metric Format (EMF) log to journal/stdout
            if let Some(ref receipt) = result.receipt {
                let emf_log = json!({
                    "_aws": {
                        "Timestamp": receipt.timestamp_ms,
                        "CloudWatchMetrics": [{
                            "Namespace": "WasmPulse/ServerlessEngine",
                            "Dimensions": [["TenantId"], ["TenantId", "FunctionName"]],
                            "Metrics": [
                                { "Name": "ColdStartLatencyUs", "Unit": "Microseconds" },
                                { "Name": "ExecutionTimeUs", "Unit": "Microseconds" },
                                { "Name": "TotalLatencyUs", "Unit": "Microseconds" },
                                { "Name": "FuelConsumed", "Unit": "Count" },
                                { "Name": "Invocations", "Unit": "Count" },
                                { "Name": "Errors", "Unit": "Count" }
                            ]
                        }]
                    },
                    "TenantId": tenant,
                    "FunctionName": function,
                    "ColdStartLatencyUs": result.cold_start_us,
                    "ExecutionTimeUs": result.execution_us,
                    "TotalLatencyUs": result.total_us,
                    "FuelConsumed": result.fuel_consumed,
                    "Invocations": 1,
                    "Errors": if result.exit_code == 0 { 0 } else { 1 },
                    "ProofHash": receipt.proof_hash,
                    "ReceiptId": receipt.receipt_id,
                });
                println!("{}", serde_json::to_string(&emf_log).unwrap_or_default());
            }

            // 2. Build HTTP response with microsecond telemetry & cryptographic proof headers
            let mut response = Response::builder()
                .status(StatusCode::OK)
                .header("X-Execution-Engine", "WasmPulse")
                .header("X-Cold-Start-Us", result.cold_start_us.to_string())
                .header("X-Execution-Time-Us", result.execution_us.to_string())
                .header("X-Total-Latency-Us", result.total_us.to_string())
                .header("X-Fuel-Consumed", result.fuel_consumed.to_string())
                .header("X-Exit-Code", result.exit_code.to_string());

            if let Some(ref receipt) = result.receipt {
                response = response
                    .header("X-Execution-Proof", &receipt.proof_hash)
                    .header("X-Execution-Receipt-Id", &receipt.receipt_id);
            }

            if !result.logs.is_empty() {
                if let Ok(serialized_logs) = serde_json::to_string(&result.logs) {
                    response = response.header("X-Sandbox-Logs", serialized_logs);
                }
            }

            let resp = response
                .body(axum::body::Body::from(result.output))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());

            Ok(resp)
        }
        Err(err) => {
            error!(tenant = %tenant, function = %function, "Function execution error: {:?}", err);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Function execution failed",
                    "details": err.to_string()
                })),
            ))
        }
    }
}
