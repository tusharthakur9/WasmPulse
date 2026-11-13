use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(name = "wasmpulse")]
#[command(about = "CLI for WasmPulse: Sub-Millisecond Wasm Serverless Execution Engine", long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "http://localhost:8080")]
    endpoint: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Wasm serverless function project template
    Init {
        /// Name of the function/directory
        name: String,
        /// Language template: rust or go
        #[arg(short, long, default_value = "rust")]
        lang: String,
    },
    /// Compile the function project into a .wasm binary
    Build {
        /// Path to the function directory (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },
    /// Deploy a compiled .wasm binary to the WasmPulse engine
    Deploy {
        /// Path to the .wasm file
        #[arg(short, long)]
        file: PathBuf,
        /// Function identifier name
        #[arg(short, long)]
        name: String,
        /// Tenant ID for multi-tenant isolation
        #[arg(short, long, default_value = "default")]
        tenant: String,
    },
    /// List all currently deployed functions on the engine
    List,
    /// Invoke a deployed serverless function
    Invoke {
        /// Name of the function to invoke
        name: String,
        /// Tenant ID
        #[arg(short, long, default_value = "default")]
        tenant: String,
        /// Optional input string / JSON payload
        #[arg(short, long, default_value = "")]
        data: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = reqwest::Client::new();

    match cli.command {
        Commands::Init { name, lang } => {
            handle_init(&name, &lang)?;
        }
        Commands::Build { path } => {
            handle_build(&path)?;
        }
        Commands::Deploy { file, name, tenant } => {
            handle_deploy(&client, &cli.endpoint, &file, &name, &tenant).await?;
        }
        Commands::List => {
            handle_list(&client, &cli.endpoint).await?;
        }
        Commands::Invoke { name, tenant, data } => {
            handle_invoke(&client, &cli.endpoint, &name, &tenant, &data).await?;
        }
    }

    Ok(())
}

fn handle_init(name: &str, lang: &str) -> Result<()> {
    let target_dir = Path::new(name);
    if target_dir.exists() {
        anyhow::bail!("Directory '{}' already exists", name);
    }
    fs::create_dir_all(target_dir.join("src"))?;

    match lang.to_lowercase().as_str() {
        "rust" => {
            let cargo_toml = format!(
                r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
"#,
                name
            );

            let lib_rs = r#"// WasmPulse Guest Function Template
extern "C" {
    fn host_log(level: i32, ptr: i32, len: i32);
    fn host_get_input_len() -> i32;
    fn host_read_input(dest_ptr: i32, max_len: i32) -> i32;
    fn host_write_output(src_ptr: i32, len: i32);
}

fn log(msg: &str) {
    unsafe { host_log(2, msg.as_ptr() as i32, msg.len() as i32) };
}

#[no_mangle]
pub extern "C" fn _start() -> i32 {
    log("Executing guest function inside WasmPulse sandbox");

    let input_len = unsafe { host_get_input_len() };
    let mut input_buffer = vec![0u8; input_len as usize];

    if input_len > 0 {
        unsafe {
            host_read_input(input_buffer.as_mut_ptr() as i32, input_len);
        }
    }

    let input_str = String::from_utf8_lossy(&input_buffer);
    let greeting = format!("Hello from WasmPulse! Received: {}", if input_str.is_empty() { "(no payload)" } else { &input_str });

    unsafe {
        host_write_output(greeting.as_ptr() as i32, greeting.len() as i32);
    }

    0 // Success
}
"#;

            fs::write(target_dir.join("Cargo.toml"), cargo_toml)?;
            fs::write(target_dir.join("src").join("lib.rs"), lib_rs)?;
            println!("Initialized Rust serverless project in './{}'", name);
            println!("To build: cd {} && wasmpulse build", name);
        }
        _ => {
            anyhow::bail!("Unsupported language template: '{}'. Supported: rust", lang);
        }
    }

    Ok(())
}

fn handle_build(path: &Path) -> Result<()> {
    println!("Building Wasm binary for target wasm32-wasip1 in {:?}...", path);
    let status = Command::new("cargo")
        .arg("build")
        .arg("--target")
        .arg("wasm32-wasip1")
        .arg("--release")
        .current_dir(path)
        .status()
        .context("Failed to execute 'cargo build --target wasm32-wasip1 --release'")?;

    if status.success() {
        println!("Build completed successfully!");
        println!("Compiled .wasm binary can be found under 'target/wasm32-wasip1/release/*.wasm'");
    } else {
        anyhow::bail!("Cargo build failed with status: {}", status);
    }
    Ok(())
}

async fn handle_deploy(
    client: &reqwest::Client,
    endpoint: &str,
    file: &Path,
    name: &str,
    tenant: &str,
) -> Result<()> {
    if !file.exists() {
        anyhow::bail!("Wasm file not found at {:?}", file);
    }

    let wasm_bytes = fs::read(file).context("Failed to read Wasm file")?;
    println!("Deploying '{}' ({} bytes) for tenant '{}' to {}...", name, wasm_bytes.len(), tenant, endpoint);

    let url = format!("{}/api/v1/functions", endpoint);
    let response = client
        .post(&url)
        .header("X-Tenant-Id", tenant)
        .header("X-Function-Name", name)
        .header("Content-Type", "application/wasm")
        .body(wasm_bytes)
        .send()
        .await
        .context("Failed to send deployment request to gateway")?;

    let status = response.status();
    let body = response.text().await?;

    if status.is_success() {
        println!("Function successfully deployed!");
        println!("{}", body);
    } else {
        eprintln!("Deployment failed (HTTP {}): {}", status, body);
    }

    Ok(())
}

async fn handle_list(client: &reqwest::Client, endpoint: &str) -> Result<()> {
    let url = format!("{}/api/v1/functions", endpoint);
    let resp = client.get(&url).send().await.context("Failed to query gateway")?;
    let body: serde_json::Value = resp.json().await?;

    println!("Deployed Serverless Functions:");
    println!("{:#}", body);
    Ok(())
}

async fn handle_invoke(
    client: &reqwest::Client,
    endpoint: &str,
    name: &str,
    tenant: &str,
    data: &str,
) -> Result<()> {
    let url = format!("{}/fn/{}/{}", endpoint, tenant, name);
    println!("Invoking function '{}/{}' at {}...", tenant, name, url);

    let t_start = std::time::Instant::now();
    let response = client
        .post(&url)
        .body(data.to_string())
        .send()
        .await
        .context("Invocation request failed")?;

    let total_wall_clock_ms = t_start.elapsed().as_secs_f64() * 1000.0;
    let headers = response.headers().clone();
    let status = response.status();
    let output = response.text().await?;

    println!("\n---------------- Execution Telemetry ----------------");
    println!("HTTP Status:         {}", status);
    if let Some(cs) = headers.get("x-cold-start-us") {
        println!("Cold Start Latency:  {} microseconds ({} ms)", cs.to_str()?, cs.to_str()?.parse::<f64>().unwrap_or(0.0) / 1000.0);
    }
    if let Some(exec) = headers.get("x-execution-time-us") {
        println!("Sandbox Execution:   {} microseconds", exec.to_str()?);
    }
    if let Some(tot) = headers.get("x-total-latency-us") {
        println!("Engine Total Time:   {} microseconds", tot.to_str()?);
    }
    if let Some(fuel) = headers.get("x-fuel-consumed") {
        println!("CPU Fuel Consumed:   {} instructions", fuel.to_str()?);
    }
    if let Some(logs) = headers.get("x-sandbox-logs") {
        println!("Sandbox Logs:        {}", logs.to_str()?);
    }
    println!("Roundtrip E2E Time:  {:.2} ms", total_wall_clock_ms);
    println!("-----------------------------------------------------");
    println!("Function Output:\n{}", output);

    Ok(())
}
