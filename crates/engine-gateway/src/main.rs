mod dashboard;
mod router;
mod state;

use anyhow::{Context, Result};
use axum::{
    routing::{delete, get, post},
    Router,
};
use engine_core::{create_pooled_engine, create_standard_engine, EngineConfig, WasmExecutor};
use engine_storage::{AotCompiler, ModuleRegistry};
use state::AppState;
use std::net::SocketAddr;
use std::path::Path;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,engine_gateway=debug,engine_core=debug".into()),
        )
        .init();

    info!("Initializing WasmPulse Serverless Engine...");

    let config = EngineConfig::default();

    // Attempt to build high-performance pooling allocator engine, fallback to standard if pool allocation fails
    let engine = match create_pooled_engine(&config) {
        Ok(e) => {
            info!("Successfully initialized High-Density Pooling Allocator Engine (10,000 instance slots)");
            e
        }
        Err(err) => {
            tracing::warn!("Failed to initialize pooled allocator (falling back to standard engine): {}", err);
            create_standard_engine().context("Failed to initialize standard engine")?
        }
    };

    let executor = WasmExecutor::new(engine.clone(), config.clone())
        .context("Failed to construct WasmExecutor")?;
    let registry = ModuleRegistry::new();

    // Auto-load reference microservice from known locations so the console is immediately operational
    let potential_paths = [
        Path::new("sample-functions/rust_echo.wasm"),
        Path::new("target/wasm32-wasip1/release/rust_echo.wasm"),
        Path::new("examples/rust-echo/target/wasm32-wasip1/release/rust_echo.wasm"),
    ];

    for wasm_path in &potential_paths {
        if wasm_path.exists() {
            if let Ok(bytes) = std::fs::read(wasm_path) {
                if let Ok(module) = AotCompiler::compile_to_module(&engine, &bytes) {
                    registry.register("my-app", "echo", bytes.len(), module);
                    info!(
                        "Pre-loaded reference function 'my-app/echo' ({} bytes) from {:?}",
                        bytes.len(),
                        wasm_path
                    );
                    break;
                }
            }
        }
    }

    let app_state = AppState::new(executor, registry, engine);

    let app = Router::new()
        .route("/", get(dashboard::render_dashboard_html))
        .route("/dashboard", get(dashboard::render_dashboard_html))
        .route("/health", get(router::health))
        .route("/metrics", get(router::metrics))
        .route("/api/v1/functions", get(router::list_functions))
        .route("/api/v1/functions", post(router::deploy_function))
        .route("/api/v1/functions/:tenant/:function", delete(router::delete_function))
        .route("/api/v1/audit/receipts", get(router::list_audit_receipts))
        .route("/api/v1/audit/verify/:receipt_id", get(router::verify_audit_receipt))
        .route("/api/v1/audit/chain", get(router::verify_audit_chain))
        .route("/fn/:tenant/:function", post(router::invoke_function))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(app_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("WasmPulse Gateway listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind TCP listener on port 8080")?;

    axum::serve(listener, app)
        .await
        .context("HTTP Gateway server error")?;

    Ok(())
}
