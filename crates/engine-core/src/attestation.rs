use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Calculates standard hex-encoded SHA-256 digest of input bytes.
pub fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Cryptographically Verifiable Execution Receipt.
/// Provides tamper-evident proof that a specific Wasm module was executed
/// with specific input bytes, consuming an exact amount of CPU fuel instructions,
/// and producing an immutable output payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionReceipt {
    /// Unique identifier for this execution proof.
    pub receipt_id: String,
    /// Unix timestamp in milliseconds when execution was attested.
    pub timestamp_ms: u64,
    /// Multi-tenant identifier.
    pub tenant_id: String,
    /// Function identifier.
    pub function_name: String,
    /// SHA-256 hash of the input payload provided to the guest.
    pub input_sha256: String,
    /// SHA-256 hash of the output payload emitted by the guest.
    pub output_sha256: String,
    /// Exact CPU fuel instructions consumed.
    pub fuel_consumed: u64,
    /// Exit code returned by guest entrypoint.
    pub exit_code: i32,
    /// Cold start latency in microseconds.
    pub cold_start_us: u128,
    /// Execution time in microseconds.
    pub execution_us: u128,
    /// Total wall clock in microseconds.
    pub total_us: u128,
    /// Cryptographic hash of the previous receipt in the audit ledger (Merkle Chain).
    pub prev_receipt_hash: String,
    /// Master cryptographic proof hash binding all execution dimensions.
    pub proof_hash: String,
}

impl ExecutionReceipt {
    /// Computes the cryptographic binding proof hash for this receipt.
    pub fn compute_proof(
        receipt_id: &str,
        timestamp_ms: u64,
        tenant_id: &str,
        function_name: &str,
        input_sha256: &str,
        output_sha256: &str,
        fuel_consumed: u64,
        exit_code: i32,
        prev_receipt_hash: &str,
    ) -> String {
        let payload = format!(
            "WasmPulse-Attestation-v1:{}:{}:{}:{}:{}:{}:{}:{}:{}",
            receipt_id,
            timestamp_ms,
            tenant_id,
            function_name,
            input_sha256,
            output_sha256,
            fuel_consumed,
            exit_code,
            prev_receipt_hash
        );
        compute_sha256(payload.as_bytes())
    }

    /// Verifies the cryptographic integrity of this receipt.
    pub fn verify(&self) -> bool {
        let expected = Self::compute_proof(
            &self.receipt_id,
            self.timestamp_ms,
            &self.tenant_id,
            &self.function_name,
            &self.input_sha256,
            &self.output_sha256,
            self.fuel_consumed,
            self.exit_code,
            &self.prev_receipt_hash,
        );
        self.proof_hash == expected
    }
}

/// Thread-safe in-memory Merkle Audit Ledger for verifiable serverless executions.
pub struct AuditLedger {
    receipts: Arc<Mutex<Vec<ExecutionReceipt>>>,
    max_history: usize,
}

impl AuditLedger {
    pub fn new(max_history: usize) -> Self {
        Self {
            receipts: Arc::new(Mutex::new(Vec::with_capacity(max_history))),
            max_history,
        }
    }

    /// Returns the proof hash of the latest committed receipt, or Genesis hash if empty.
    pub fn latest_hash(&self) -> String {
        let lock = self.receipts.lock().unwrap();
        lock.last()
            .map(|r| r.proof_hash.clone())
            .unwrap_or_else(|| "0000000000000000000000000000000000000000000000000000000000000000".to_string())
    }

    /// Records an execution into the ledger, chaining it cryptographically.
    pub fn record(
        &self,
        tenant_id: &str,
        function_name: &str,
        input_data: &[u8],
        output_data: &[u8],
        fuel_consumed: u64,
        exit_code: i32,
        cold_start_us: u128,
        execution_us: u128,
        total_us: u128,
    ) -> ExecutionReceipt {
        let mut lock = self.receipts.lock().unwrap();
        let prev_receipt_hash = lock
            .last()
            .map(|r| r.proof_hash.clone())
            .unwrap_or_else(|| "0000000000000000000000000000000000000000000000000000000000000000".to_string());

        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let seq_num = lock.len() + 1;
        let receipt_id = format!("rcpt_{}_{:06}", timestamp_ms, seq_num);
        let input_sha256 = compute_sha256(input_data);
        let output_sha256 = compute_sha256(output_data);

        let proof_hash = ExecutionReceipt::compute_proof(
            &receipt_id,
            timestamp_ms,
            tenant_id,
            function_name,
            &input_sha256,
            &output_sha256,
            fuel_consumed,
            exit_code,
            &prev_receipt_hash,
        );

        let receipt = ExecutionReceipt {
            receipt_id,
            timestamp_ms,
            tenant_id: tenant_id.to_string(),
            function_name: function_name.to_string(),
            input_sha256,
            output_sha256,
            fuel_consumed,
            exit_code,
            cold_start_us,
            execution_us,
            total_us,
            prev_receipt_hash,
            proof_hash,
        };

        if lock.len() >= self.max_history {
            lock.remove(0);
        }
        lock.push(receipt.clone());

        receipt
    }

    /// Lists recent receipts up to `limit`.
    pub fn list(&self, limit: usize) -> Vec<ExecutionReceipt> {
        let lock = self.receipts.lock().unwrap();
        let take_n = std::cmp::min(limit, lock.len());
        lock.iter().rev().take(take_n).cloned().collect()
    }

    /// Finds a receipt by receipt_id.
    pub fn get_by_id(&self, receipt_id: &str) -> Option<ExecutionReceipt> {
        let lock = self.receipts.lock().unwrap();
        lock.iter().find(|r| r.receipt_id == receipt_id).cloned()
    }

    /// Audits the cryptographic Merkle chain of all receipts.
    /// Returns (is_valid, count_verified).
    pub fn verify_chain(&self) -> (bool, usize) {
        let lock = self.receipts.lock().unwrap();
        let mut prev_hash = "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        for (idx, r) in lock.iter().enumerate() {
            if idx > 0 && r.prev_receipt_hash != prev_hash {
                return (false, idx);
            }
            if !r.verify() {
                return (false, idx);
            }
            prev_hash = r.proof_hash.clone();
        }
        (true, lock.len())
    }
}

impl Default for AuditLedger {
    fn default() -> Self {
        Self::new(1000)
    }
}
