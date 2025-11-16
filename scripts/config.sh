#!/bin/bash

# Central configuration for all deployment scripts

# AWS Configuration
export AWS_REGION="${AWS_REGION:-eu-north-1}"
export AWS_PROFILE="${AWS_PROFILE:-default}"

# EC2 Configuration
export EC2_HOST="${EC2_HOST:-13.61.166.212}"
export EC2_USER="${EC2_USER:-ubuntu}"
export EC2_KEY_FILE="${EC2_KEY_FILE:-$HOME/Downloads/key.pem}"

# Deployment Configuration
export DEPLOY_DIR="${DEPLOY_DIR:-/opt/trading-bot}"
export BACKUP_RETENTION_DAYS="${BACKUP_RETENTION_DAYS:-7}"

# GitHub Configuration
export GITHUB_REPO="${GITHUB_REPO:-your-username/trading-bot-pro}"

# S3 Bucket for deployments (optional)
export S3_BUCKET="${S3_BUCKET:-your-deployment-bucket}"

# Monitoring
export PROMETHEUS_PORT="${PROMETHEUS_PORT:-9090}"
export GRAFANA_PORT="${GRAFANA_PORT:-3001}"
export FRONTEND_PORT="${FRONTEND_PORT:-3000}"

validate_config() {
  if [ "$EC2_HOST" = "YOUR_INSTANCE_IP_HERE" ]; then
    echo "❌ Error: EC2_HOST not configured!"
    echo "Edit scripts/config.sh and set your EC2 instance IP."
    exit 1
  fi

  if [ ! -f "$EC2_KEY_FILE" ]; then
    echo "❌ Error: SSH key not found at $EC2_KEY_FILE"
    exit 1
  fi

  chmod 600 "$EC2_KEY_FILE"
}

