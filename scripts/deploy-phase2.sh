#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/config.sh"
validate_config

echo "🚀 Phase 2 Deployment - Building on EC2..."

# Package source code (no binary)
ARTIFACT="source.tar.gz"
echo "📦 Packaging source code..."
tar -czf "$ARTIFACT" \
  --exclude=target \
  src/ \
  Cargo.toml \
  Cargo.lock \
  .env.example

# Upload
echo "📤 Uploading to server..."
scp -i "$EC2_KEY_FILE" "$ARTIFACT" "${EC2_USER}@${EC2_HOST}:/tmp/"

# Deploy and build on EC2
echo "🔧 Building and deploying on EC2..."
ssh -i "$EC2_KEY_FILE" "${EC2_USER}@${EC2_HOST}" <<EOF
  set -euo pipefail
  DEPLOY_DIR="$DEPLOY_DIR"
  FRONTEND_PORT="$FRONTEND_PORT"

  # Install build tools if not present
  if ! command -v gcc &> /dev/null; then
    echo "📥 Installing build tools..."
    sudo apt-get update
    sudo apt-get install -y build-essential pkg-config libssl-dev
  fi

  # Install Rust if not present
  if ! command -v cargo &> /dev/null; then
    echo "📥 Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "\$HOME/.cargo/env"
  fi

  # Create deployment directory
  sudo mkdir -p "\$DEPLOY_DIR"
  sudo chown -R ubuntu:ubuntu "\$DEPLOY_DIR"
  cd "\$DEPLOY_DIR"

  # Backup existing
  if [ -d "current" ]; then
    sudo mv current "backup-\$(date +%Y%m%d-%H%M%S)"
  fi

  # Extract source
  mkdir -p current
  cd current
  tar -xzf /tmp/$ARTIFACT

  # Build on EC2 (native Linux build)
  echo "🔨 Building on EC2..."
  source "\$HOME/.cargo/env"
  cargo build --release

  # Create systemd service
  sudo tee /etc/systemd/system/trading-bot.service > /dev/null <<SERVICE
[Unit]
Description=KuCoin Ultimate Trading Bot
After=network.target

[Service]
Type=simple
User=ubuntu
WorkingDirectory=\$DEPLOY_DIR/current
Environment="RUST_LOG=info"
EnvironmentFile=\$DEPLOY_DIR/.env
ExecStart=\$DEPLOY_DIR/current/target/release/kucoin-ultimate-trading-bot
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
SERVICE

  # Create .env if not exists
  if [ ! -f "\$DEPLOY_DIR/.env" ]; then
    echo "📝 Creating .env file..."
    cp .env.example "\$DEPLOY_DIR/.env"
    echo "⚠️  Please update \$DEPLOY_DIR/.env with real credentials!"
  fi

  # Start service
  sudo systemctl daemon-reload
  sudo systemctl enable trading-bot
  sudo systemctl restart trading-bot

  # Wait and check
  sleep 5
  echo ""
  echo "📊 Service status:"
  sudo systemctl status trading-bot --no-pager | head -20

  echo ""
  echo "🏥 Testing health endpoint..."
  sleep 2
  curl -f "http://localhost:\$FRONTEND_PORT/health" | jq . || echo "⚠️  Health check failed"

  echo ""
  echo "✅ Deployment complete!"
EOF

echo ""
echo "✅ Phase 2 deployment complete!"
echo "🌐 Health endpoint: http://$EC2_HOST:$FRONTEND_PORT/health"
echo ""
echo "📋 Useful commands:"
echo "  Check logs:    ssh -i $EC2_KEY_FILE ${EC2_USER}@${EC2_HOST} 'sudo journalctl -u trading-bot -f'"
echo "  Check status:  ssh -i $EC2_KEY_FILE ${EC2_USER}@${EC2_HOST} 'sudo systemctl status trading-bot'"
echo "  Restart:       ssh -i $EC2_KEY_FILE ${EC2_USER}@${EC2_HOST} 'sudo systemctl restart trading-bot'"

# Cleanup
rm -f "$ARTIFACT"

