#!/bin/bash
# ==============================================================================
# WasmPulse ⚡ AWS CloudWatch Integration Installer
# Installs Amazon CloudWatch Agent & Links WasmPulse Journal Logs & EMF Metrics
# ==============================================================================

set -e

echo "=========================================================="
echo "☁️ Installing AWS CloudWatch Unified Agent on EC2..."
echo "=========================================================="

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_FILE="${SCRIPT_DIR}/amazon-cloudwatch-agent.json"

# 1. Download & Install Amazon CloudWatch Agent for Ubuntu
if ! command -v amazon-cloudwatch-agent-ctl &> /dev/null; then
    echo "--> [1/4] Downloading Amazon CloudWatch Agent .deb package..."
    wget -q https://s3.amazonaws.com/amazoncloudwatch-agent/ubuntu/amd64/latest/amazon-cloudwatch-agent.deb -O /tmp/amazon-cloudwatch-agent.deb
    echo "--> [2/4] Installing package..."
    sudo dpkg -i -E /tmp/amazon-cloudwatch-agent.deb
    rm -f /tmp/amazon-cloudwatch-agent.deb
else
    echo "--> CloudWatch Agent already installed."
fi

# 2. Setup Log redirection from systemd service to /var/log/wasmpulse.log
echo "--> [3/4] Piping WasmPulse systemd journal logs to /var/log/wasmpulse.log..."
sudo bash -c "journalctl -u wasmpulse -f >> /var/log/wasmpulse.log 2>&1 &" || true

# 3. Apply Configuration and Start CloudWatch Agent
echo "--> [4/4] Starting CloudWatch Agent with WasmPulse configuration..."
sudo /opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-ctl \
    -a fetch-config \
    -m ec2 \
    -c file:"${CONFIG_FILE}" \
    -s

echo ""
echo "=========================================================="
echo "🎉 CloudWatch Agent is ACTIVE & STREAMING to AWS Console!"
echo "=========================================================="
echo "📊 CloudWatch Log Group:   /aws/ec2/wasmpulse"
echo "📈 CloudWatch Metrics:     Namespace 'WasmPulse/HostMetrics' & 'WasmPulse/ServerlessEngine'"
echo "🧭 View your metrics:      AWS Console -> CloudWatch -> Metrics -> All metrics"
echo "=========================================================="
