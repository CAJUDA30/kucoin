#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/config.sh"
validate_config

echo "⏮️  Rolling back to previous version..."

ssh -i "$EC2_KEY_FILE" "${EC2_USER}@${EC2_HOST}" <<EOF
  set -euo pipefail
  DEPLOY_DIR="$DEPLOY_DIR"
  FRONTEND_PORT="$FRONTEND_PORT"

  cd "\$DEPLOY_DIR"

  BACKUP=\$(ls -t | grep backup- | head -1)

  if [ -z "\$BACKUP" ]; then
    echo "❌ No backup found!"
    exit 1
  fi

  echo "Rolling back to: \$BACKUP"

  cd "\$DEPLOY_DIR/current/docker"
  docker compose down || true

  cd "\$DEPLOY_DIR"
  mv current "failed-deploy-\$(date +%Y%m%d-%H%M%S)"
  mv "\$BACKUP" current

  cd current/docker
  docker compose up -d

  sleep 10
  curl -f "http://localhost:\$FRONTEND_PORT/health" || exit 1

  echo "✅ Rollback successful!"
EOF

echo "✅ Rollback complete!"
echo "🌐 Dashboard: http://$EC2_HOST:$FRONTEND_PORT"
