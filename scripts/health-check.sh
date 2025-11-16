#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/config.sh"
validate_config

echo "🏥 Health Check Report - $(date)"
echo "================================"

echo
echo "📊 Bot Status:"
ssh -i "$EC2_KEY_FILE" "${EC2_USER}@${EC2_HOST}" <<EOF
  set -e
  DEPLOY_DIR="$DEPLOY_DIR"
  if [ -d "\$DEPLOY_DIR/current/docker" ]; then
    cd "\$DEPLOY_DIR/current/docker"
    docker compose ps 2>/dev/null || docker ps
  else
    echo "⚠️  No deployment found (current/docker directory does not exist)"
  fi
EOF

echo
echo "🌐 API Health:"
if ! curl -s "http://${EC2_HOST}:${FRONTEND_PORT}/health" | jq .; then
  echo "API not responding"
fi

echo
echo "📈 Prometheus Status:"
if ! curl -s "http://${EC2_HOST}:${PROMETHEUS_PORT}/-/healthy"; then
  echo "Prometheus not responding"
fi

echo
echo "💾 Disk Usage:"
ssh -i "$EC2_KEY_FILE" "${EC2_USER}@${EC2_HOST}" "df -h $DEPLOY_DIR"

echo
echo "❌ Recent Errors (last 10):"
ssh -i "$EC2_KEY_FILE" "${EC2_USER}@${EC2_HOST}" <<EOF
  DEPLOY_DIR="$DEPLOY_DIR"
  if [ -d "\$DEPLOY_DIR/current/docker" ]; then
    cd "\$DEPLOY_DIR/current/docker"
    docker compose logs --tail=100 2>/dev/null | grep -i error | tail -10 || echo "No recent errors"
  else
    echo "No deployment to check logs for"
  fi
EOF

echo
echo "✅ Health check complete"
