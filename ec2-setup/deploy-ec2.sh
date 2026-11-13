#!/bin/bash
# ==============================================================================
# WasmPulse ⚡ AWS EC2 One-Click Production Deployment Script
# Supports: Ubuntu 22.04 / 24.04 LTS & Debian 11/12
# ==============================================================================

set -e

# Resolve absolute path to project root regardless of where this script is called from
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${ROOT_DIR}"

echo "=========================================================="
echo "⚡ Starting WasmPulse Serverless Engine Setup on AWS EC2..."
echo "📂 Project Root: ${ROOT_DIR}"
echo "=========================================================="

# 1. Update system packages and install prerequisites
echo "--> [1/6] Updating system packages & installing build tools..."
sudo apt-get update -y
sudo apt-get install -y curl build-essential pkg-config libssl-dev git ufw jq

# Ensure sufficient swap memory for small EC2 instances (t2.micro / t3.micro have only 1GB RAM)
MEM_MB=$(free -m | awk '/^Mem:/{print $2}')
SWAP_MB=$(free -m | awk '/^Swap:/{print $2}')
if [ "${MEM_MB:-1000}" -lt 2500 ] && [ "${SWAP_MB:-0}" -lt 1000 ]; then
    echo "--> [RAM Check] Low RAM detected (${MEM_MB}MB). Creating 4GB swap space to prevent OOM Killer..."
    sudo fallocate -l 4G /swapfile || sudo dd if=/dev/zero of=/swapfile bs=1M count=4096
    sudo chmod 600 /swapfile
    sudo mkswap /swapfile
    sudo swapon /swapfile
    echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab || true
    echo "--> 4GB swap space activated successfully."
fi

# 2. Install Rust toolchain if not present
if ! command -v cargo &> /dev/null; then
    echo "--> [2/6] Installing Rust toolchain..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
else
    echo "--> [2/6] Rust already installed."
    source "$HOME/.cargo/env" || true
fi

# 3. Add Wasm compilation targets
echo "--> [3/6] Adding wasm32-wasip1 compilation target..."
rustup target add wasm32-wasip1

# 4. Build Engine & Sample Functions in Release Mode
echo "--> [4/6] Compiling WasmPulse Engine (using single job to conserve memory)..."
cargo build --release -p engine-gateway -j 1
cargo build --release -p wasmpulse-cli -j 1

# Precompile sample function if source exists
mkdir -p target/wasm32-wasip1/release
if [ -d "examples/rust-echo" ]; then
    echo "--> Compiling sample guest microservice (rust-echo)..."
    cargo build -p rust-echo --target wasm32-wasip1 --release || true
fi

# Ensure reference wasm function is placed at both expected locations
mkdir -p sample-functions
if [ -f "examples/rust-echo/target/wasm32-wasip1/release/rust_echo.wasm" ]; then
    cp examples/rust-echo/target/wasm32-wasip1/release/rust_echo.wasm target/wasm32-wasip1/release/rust_echo.wasm
    cp examples/rust-echo/target/wasm32-wasip1/release/rust_echo.wasm sample-functions/rust_echo.wasm
elif [ -f "sample-functions/rust_echo.wasm" ]; then
    cp sample-functions/rust_echo.wasm target/wasm32-wasip1/release/rust_echo.wasm
fi

# 5. Setup Systemd Service for 24/7 background operation
echo "--> [5/6] Configuring 24/7 background systemd service..."
CURRENT_USER=$(whoami)

sudo bash -c "cat <<EOF > /etc/systemd/system/wasmpulse.service
[Unit]
Description=WasmPulse Sub-Millisecond Serverless Engine
After=network.target

[Service]
Type=simple
User=${CURRENT_USER}
WorkingDirectory=${ROOT_DIR}
ExecStart=${ROOT_DIR}/target/release/engine-gateway
Restart=always
RestartSec=3
LimitNOFILE=65535
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF"

sudo systemctl daemon-reload
sudo systemctl enable wasmpulse
sudo systemctl restart wasmpulse

# 6. Configure Firewall to allow port 8080 and 22
echo "--> [6/6] Configuring firewall rules (8080/tcp, 22/tcp)..."
sudo ufw allow 8080/tcp || true
sudo ufw allow 22/tcp || true

# Fetch Public IP with fallback mechanisms
PUBLIC_IP=$(curl -s --connect-timeout 3 http://checkip.amazonaws.com || curl -s --connect-timeout 3 https://ifconfig.me || echo "YOUR-EC2-PUBLIC-IP")

echo ""
echo "=========================================================="
echo "🎉 SUCCESS! WasmPulse is now running live on your AWS EC2 node!"
echo "=========================================================="
echo ""
echo "👉 Web Console:    http://${PUBLIC_IP}:8080"
echo "👉 Metrics API:    http://${PUBLIC_IP}:8080/metrics"
echo "👉 Health Status:  http://${PUBLIC_IP}:8080/health"
echo "👉 Verifiable Audit: http://${PUBLIC_IP}:8080/api/v1/audit/receipts"
echo ""
echo "To check live server logs:"
echo "  sudo journalctl -u wasmpulse -f"
echo ""
echo "To restart server:"
echo "  sudo systemctl restart wasmpulse"
echo ""
echo "To setup AWS CloudWatch Logs & Metrics Integration:"
echo "  cd ${ROOT_DIR}/ec2-setup && ./setup-cloudwatch.sh"
echo "=========================================================="
