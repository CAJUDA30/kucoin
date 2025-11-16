#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/config.sh"
validate_config

echo "🚀 Starting simplified deployment to $EC2_HOST..."

# Download latest Linux binary from GitHub Actions
echo "📥 Downloading latest Linux binary from GitHub Actions..."
gh run download \
  --repo CAJUDA30/kucoin \
  --name trading-bot-binary

chmod +x kucoin-ultimate-trading-bot

# Upload to EC2
echo "📤 Uploading binary to EC2..."
scp -i "$EC2_KEY_FILE" kucoin-ultimate-trading-bot ${EC2_USER}@${EC2_HOST}:/tmp/

# Deploy
echo "🔧 Deploying on EC2..."
ssh -i "$EC2_KEY_FILE" ${EC2_USER}@${EC2_HOST} << 'REMOTE'
  mkdir -p /opt/trading-bot
  cd /opt/trading-bot
  
  # Install binary
  mv /tmp/kucoin-ultimate-trading-bot .
  chmod +x kucoin-ultimate-trading-bot
  
  # Create .env
  cat > .env << 'ENV'
KUCOIN_SANDBOX_MODE=true
FRONTEND_PORT=3000
LOG_LEVEL=info
PROMETHEUS_PORT=9090
GRAFANA_PORT=3001
ENV
  
  # Stop old, start new
  pkill -f kucoin-ultimate || true
  nohup ./kucoin-ultimate-trading-bot > bot.log 2>&1 &
  
  sleep 5
  echo ""
  echo "🏥 Health check:"
  curl -s http://localhost:3000/health | jq . || echo "Health endpoint not responding yet"
REMOTE

echo ""
echo "✅ Deployed! Check: http://$EC2_HOST:3000/health"
rm -f kucoin-ultimate-trading-bot

echo ""
echo "📊 To view logs:"
echo "ssh -i $EC2_KEY_FILE ${EC2_USER}@${EC2_HOST} 'tail -f /opt/trading-bot/bot.log'"

