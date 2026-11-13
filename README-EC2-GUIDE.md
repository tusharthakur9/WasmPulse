# WasmPulse ⚡ AWS EC2 & CloudWatch Production Management Guide

Complete step-by-step handbook to deploy, monitor, and operate the **WasmPulse Serverless Engine** on **Amazon Web Services (AWS)** using the AWS Management Console and automated scripts.

---

## 📑 Table of Contents
1. [Architecture Overview](#-architecture-overview)
2. [Step 1: Launch & Configure AWS EC2 Instance](#step-1-launch--configure-aws-ec2-instance)
3. [Step 2: Create & Attach CloudWatch IAM Role in AWS Console](#step-2-create--attach-cloudwatch-iam-role-in-aws-console)
4. [Step 3: Deploy WasmPulse & CloudWatch Agent](#step-3-deploy-wasmpulse--cloudwatch-agent)
5. [Step 4: Manage & View Metrics in AWS CloudWatch Console](#step-4-manage--view-metrics-in-aws-cloudwatch-console)
6. [Step 5: Import CloudWatch Production Dashboard](#step-5-import-cloudwatch-production-dashboard)
7. [Step 6: Configure CloudWatch Alarms & SNS Email Alerts](#step-6-configure-cloudwatch-alarms--sns-email-alerts)
8. [Verifiable Audit Ledger & Cryptographic Proof Verification](#-verifiable-audit-ledger--cryptographic-proof-verification)
9. [Operational Commands Cheat Sheet](#-operational-commands-cheat-sheet)

---

## 🏛 Architecture Overview

```
                      ┌────────────────────────────────────────┐
                      │              CLIENT / WEB              │
                      └──────────────────┬─────────────────────┘
                                         │ HTTP Invocations (Port 8080)
                                         ▼
                      ┌────────────────────────────────────────┐
                      │       AWS EC2 (Ubuntu 22.04 LTS)       │
                      │                                        │
                      │  ┌──────────────────────────────────┐  │
                      │  │   WasmPulse Gateway (Axum)       │  │
                      │  │   - Sub-millisecond router       │  │
                      │  │   - Pooling virtual memory       │  │
                      │  │   - Merkle Audit Ledger          │  │
                      │  └──────────────────┬───────────────┘  │
                      │                     │                  │
                      │                     │ Native EMF JSON  │
                      │                     ▼                  │
                      │  ┌──────────────────────────────────┐  │
                      │  │     Amazon CloudWatch Agent      │  │
                      │  │     (Streams OS & Engine logs)   │  │
                      │  └──────────────────┬───────────────┘  │
                      └─────────────────────┼──────────────────┘
                                            │ IAM Role (CloudWatchAgentServerPolicy)
                                            ▼
                      ┌────────────────────────────────────────┐
                      │             AWS CLOUDWATCH             │
                      │  - Namespace: WasmPulse/ServerlessEngine│
                      │  - Microsecond Latency Dashboards      │
                      │  - CPU Fuel Consumption Metrics        │
                      │  - SNS Email Alarm Notifications       │
                      └────────────────────────────────────────┘
```

---

## Step 1: Launch & Configure AWS EC2 Instance

1. Log in to the [AWS Management Console](https://console.aws.amazon.com/) and navigate to **EC2**.
2. Click **Launch Instance** (top right) and set the following parameters:
   * **Name and tags**: `WasmPulse-Production-Engine`
   * **Application and OS Images (Amazon Machine Image)**: Select **Ubuntu Server 22.04 LTS** or **Ubuntu 24.04 LTS** (Free Tier Eligible, 64-bit x86).
   * **Instance type**: `t2.micro` or `t3.micro` (Free Tier eligible; `t3.small` or `c6i.large` recommended for high concurrency).
   * **Key pair (login)**: Select your key pair or click **Create new key pair** (RSA, `.pem`), save it as `wasmpulse-key.pem`.
3. **Network settings**:
   * Click **Edit** (top right of Network settings).
   * Ensure **Auto-assign public IP** is set to **Enable**.
   * Under **Firewall (security groups)**, select **Create security group**:
     * **Security group name**: `wasmpulse-sg`
     * **Description**: `Inbound rules for WasmPulse Engine and SSH`
     * **Inbound rule 1 (SSH)**:
       * Type: `SSH` | Protocol: `TCP` | Port range: `22` | Source: `My IP` (or `0.0.0.0/0`)
     * **Inbound rule 2 (WasmPulse HTTP Gateway)**:
       * Type: `Custom TCP` | Protocol: `TCP` | Port range: `8080` | Source: `Anywhere-IPv4` (`0.0.0.0/0`) | Description: `WasmPulse Web Console and API`
4. Click **Launch Instance** and wait ~30 seconds until the instance state shows **Running**.

---

## Step 2: Create & Attach CloudWatch IAM Role in AWS Console

To allow your EC2 instance to send logs and metrics into AWS CloudWatch, attach an IAM Role.

### 2.1 Create the IAM Role
1. In the AWS Management Console search bar, type **IAM** and select **IAM**.
2. In the left navigation pane, click **Roles**, then click **Create role**.
3. Under **Trusted entity type**, select **AWS service**.
4. Under **Use case**, select **EC2**, then click **Next**.
5. In the **Permissions policies** filter box, type:
   ```text
   CloudWatchAgentServerPolicy
   ```
6. Check the box next to `CloudWatchAgentServerPolicy` and click **Next**.
7. In **Role name**, enter:
   ```text
   WasmPulse-CloudWatch-Role
   ```
8. Click **Create role**.

### 2.2 Attach Role to Your EC2 Instance
1. Go back to **EC2 > Instances**.
2. Select your `WasmPulse-Production-Engine` instance checkbox.
3. Click **Actions** (top right) > **Security** > **Modify IAM role**.
4. In the **IAM role** dropdown, select **`WasmPulse-CloudWatch-Role`**.
5. Click **Update IAM role**.

---

## Step 3: Deploy WasmPulse & CloudWatch Agent

### 3.1 Copy Files to EC2
Open your local PowerShell or Terminal and run (replace `wasmpulse-key.pem` and `YOUR-EC2-PUBLIC-IP`):
```powershell
# From local machine:
scp -i "C:\path\to\wasmpulse-key.pem" -r "C:\Users\HP\OneDrive\Desktop\WasmPulse-Serverless" ubuntu@YOUR-EC2-PUBLIC-IP:~/
```

### 3.2 SSH into the Instance
```bash
ssh -i "path/to/wasmpulse-key.pem" ubuntu@YOUR-EC2-PUBLIC-IP
```

### 3.3 Run Automated Deployment
```bash
cd ~/WasmPulse-Serverless/ec2-setup
chmod +x deploy-ec2.sh setup-cloudwatch.sh
./deploy-ec2.sh
```

### 3.4 Install & Start Amazon CloudWatch Agent
```bash
sudo ./setup-cloudwatch.sh
```

**Verification:**
* Check WasmPulse engine: `sudo systemctl status wasmpulse` (should show `active (running)`)
* Check CloudWatch Agent: `sudo /opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-ctl -a status` (should show `status: running`)
* Open in browser: `http://YOUR-EC2-PUBLIC-IP:8080`

---

## Step 4: Manage & View Metrics in AWS CloudWatch Console

WasmPulse emits **Embedded Metric Format (EMF)** JSON logs on every invocation. CloudWatch automatically ingests them into high-resolution metrics without custom code.

1. In the AWS Console, navigate to **CloudWatch**.
2. In the left sidebar, click **Metrics** > **All metrics**.
3. Under **Custom Namespaces**, click **`WasmPulse/ServerlessEngine`**:
   * **By `TenantId, FunctionName`**:
     * `ColdStartLatencyUs`: Real-time cold start in microseconds (~150 µs).
     * `ExecutionTimeUs`: Microsecond execution duration (~80 µs).
     * `TotalLatencyUs`: Complete end-to-end sandbox time (~230 µs).
     * `FuelConsumed`: Exact deterministic CPU instructions consumed.
   * **By `TenantId`**:
     * `Invocations`: Total invocations count.
     * `Errors`: Non-zero exit code counter.
4. Click any metric checkbox to graph it live in the CloudWatch Grapher.

### Live Logs in CloudWatch
1. In CloudWatch sidebar, click **Logs** > **Log groups**.
2. Click **`/aws/ec2/wasmpulse`** or **`/aws/ec2/wasmpulse/engine.log`**.
3. View real-time log streams containing invocation timestamps, memory pool slot allocations, and cryptographic receipts.

---

## Step 5: Import CloudWatch Production Dashboard

WasmPulse includes a pre-configured, production-ready CloudWatch dashboard with 6 widgets (latency charts, fuel consumption, invocation rate, and live logs).

1. In the AWS Console, open **CloudWatch** > **Dashboards**.
2. Click **Create dashboard**.
3. Name: `WasmPulse-Executive-Dashboard` and click **Create dashboard**.
4. In the "Add widget" popup, click **Cancel**.
5. In the top-right toolbar, click **Actions** > **View/edit source**.
6. Replace the existing JSON with the entire contents of `ec2-setup/cloudwatch-dashboard.json`:
   *(You can also click the "📋 Copy CloudWatch Dashboard JSON" button in the WasmPulse Web Console tab "☁️ AWS CloudWatch & EC2")*
7. Click **Update** and then **Save dashboard**.
8. You now have an executive-grade dashboard rendering live microsecond serverless telemetry!

---

## Step 6: Configure CloudWatch Alarms & SNS Email Alerts

Set up automated alerts for high latency or execution errors:

1. In the CloudWatch sidebar, click **Alarms** > **All alarms** > **Create alarm**.
2. Click **Select metric** > `WasmPulse/ServerlessEngine` > `TenantId, FunctionName`.
3. Select `TotalLatencyUs` for `my-app / echo`, click **Select metric**.
4. Configure conditions:
   * **Statistic**: `Average` (or `p99`)
   * **Period**: `10 seconds` (or `1 minute`)
   * **Threshold type**: `Static`
   * **Whenever TotalLatencyUs is...**: `Greater than > 5000` (5 milliseconds SLA breach).
5. Click **Next**.
6. Under **Notification**:
   * Select **In alarm**.
   * Send notification to: **Create new topic**.
   * Topic name: `WasmPulse-SLA-Breach`
   * Email endpoints: Enter your email address.
   * Click **Create topic**.
7. Confirm the subscription in your email inbox!
8. Click **Next**, name the alarm `WasmPulse-HighLatency-Warning`, and click **Create alarm**.

---

## 🛡️ Verifiable Audit Ledger & Cryptographic Proof Verification

WasmPulse introduces **Cryptographically Verifiable Serverless Execution with Merkle Audit Chains**, an unprecedented capability for compliance, financial transactions, and zero-trust computing.

### How It Works
1. Every time `/fn/:tenant/:function` is invoked:
   * Input bytes are hashed: $H_{in} = \text{SHA-256}(Input)$
   * Guest execution produces output: $H_{out} = \text{SHA-256}(Output)$
   * Exact CPU fuel instructions are metered deterministically.
   * A cryptographic receipt is computed:
     $$\text{ReceiptHash} = \text{SHA-256}(\text{ReceiptID} \parallel \text{Tenant} \parallel \text{Func} \parallel H_{in} \parallel H_{out} \parallel \text{Fuel} \parallel \text{ExitCode} \parallel \text{PrevReceiptHash})$$
   * Receipts are appended to a stateful Merkle Audit Chain.

### Verifying Proofs via API
* **List receipts:**
  ```bash
  curl http://YOUR-EC2-PUBLIC-IP:8080/api/v1/audit/receipts
  ```
* **Verify single execution receipt:**
  ```bash
  curl http://YOUR-EC2-PUBLIC-IP:8080/api/v1/audit/verify/rcpt_1731475200000_1
  # Output: {"is_valid": true, "proof_hash": "...", "receipt": {...}}
  ```
* **Verify entire Merkle audit chain:**
  ```bash
  curl http://YOUR-EC2-PUBLIC-IP:8080/api/v1/audit/chain
  # Output: {"chain_valid": true, "receipts_verified": 42, "latest_proof_hash": "..."}
  ```

---

## 🔄 Operational Commands Cheat Sheet

| Action | Command |
| :--- | :--- |
| **View live engine logs** | `sudo journalctl -u wasmpulse -f` |
| **Check service status** | `sudo systemctl status wasmpulse` |
| **Restart engine** | `sudo systemctl restart wasmpulse` |
| **Stop engine** | `sudo systemctl stop wasmpulse` |
| **Check CloudWatch Agent** | `sudo /opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-ctl -a status` |
| **Restart CloudWatch Agent**| `sudo /opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-ctl -a stop && sudo /opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-ctl -a start` |
| **Prometheus metrics dump** | `curl http://localhost:8080/metrics?format=prometheus` |
| **JSON metrics dump** | `curl http://localhost:8080/metrics` |

---

✅ **Your WasmPulse Serverless Engine is now fully integrated with AWS EC2 and Amazon CloudWatch!**
