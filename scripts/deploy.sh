#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/config.sh"
validate_config

ARTIFACT="deploy.tar.gz"

echo "🚀 Starting deployment to $EC2_HOST..."

# Build locally
echo "📦 Building release..."
cargo build --release

# Build frontend
echo "🎨 Building frontend..."
pushd frontend >/dev/null
npm run build
popd >/dev/null

# Package
echo "📦 Packaging..."
tar -czf "$ARTIFACT" \
  target/release/kucoin-ultimate-trading-bot \
  docker/ \
  frontend/build/ \
  .env.example

# Upload
echo "📤 Uploading to server..."
scp -i "$EC2_KEY_FILE" "$ARTIFACT" "${EC2_USER}@${EC2_HOST}:/tmp/"

# Deploy
echo "🔧 Deploying on server..."
ssh -i "$EC2_KEY_FILE" "${EC2_USER}@${EC2_HOST}" <<EOF
  set -euo pipefail
  DEPLOY_DIR="$DEPLOY_DIR"
  FRONTEND_PORT="$FRONTEND_PORT"

  cd "\$DEPLOY_DIR"

  # Backup
  if [ -d "current" ]; then
    sudo mv current "backup-\$(date +%Y%m%d-%H%M%S)"
  fi

  # Extract
  mkdir -p current
  cd current
  tar -xzf /tmp/$ARTIFACT

  # Copy production env
  if [ -f "\$DEPLOY_DIR/.env" ]; then
    cp "\$DEPLOY_DIR/.env" .env
  fi

  # Restart
  cd docker
  docker-compose down
  docker-compose up -d

  # Health check
  sleep 10
  curl -f "http://localhost:\$FRONTEND_PORT/health" || exit 1

  echo "✅ Deployment successful!"
EOF

echo "✅ Deployment complete!"
echo "🌐 Dashboard: http://$EC2_HOST:$FRONTEND_PORT"

# Cleanup
rm -f "$ARTIFACT"
