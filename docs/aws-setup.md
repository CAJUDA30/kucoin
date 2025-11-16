# AWS Infrastructure Setup Guide

Follow these steps to provision infrastructure for the trading bot. All commands assume you have valid AWS credentials with permissions for EC2, VPC networking, IAM, and S3.

## 1. Configure AWS CLI
```bash
aws configure
# Provide AWS Access Key ID, Secret Access Key, default region (us-east-1), and output format (json)
```

## 2. Key Pair
```bash
aws ec2 create-key-pair \
  --key-name trading-bot-key \
  --query 'KeyMaterial' \
  --output text > ~/trading-bot-key.pem
chmod 400 ~/trading-bot-key.pem
```

## 3. Security Group
```bash
aws ec2 create-security-group \
  --group-name trading-bot-sg \
  --description "Security group for trading bot"

SG_ID=$(aws ec2 describe-security-groups \
  --group-names trading-bot-sg \
  --query 'SecurityGroups[0].GroupId' \
  --output text)

aws ec2 authorize-security-group-ingress --group-id $SG_ID --protocol tcp --port 22   --cidr YOUR_IP/32
aws ec2 authorize-security-group-ingress --group-id $SG_ID --protocol tcp --port 80   --cidr 0.0.0.0/0
aws ec2 authorize-security-group-ingress --group-id $SG_ID --protocol tcp --port 443  --cidr 0.0.0.0/0
aws ec2 authorize-security-group-ingress --group-id $SG_ID --protocol tcp --port 3000 --cidr YOUR_IP/32
aws ec2 authorize-security-group-ingress --group-id $SG_ID --protocol tcp --port 9090 --cidr YOUR_IP/32
```

## 4. Launch EC2 Instance
```bash
aws ec2 run-instances \
  --image-id ami-0c7217cdde317cfec \
  --count 1 \
  --instance-type t3.xlarge \
  --key-name trading-bot-key \
  --security-group-ids $SG_ID \
  --subnet-id subnet-xxxxxxxxx \
  --tag-specifications 'ResourceType=instance,Tags=[{Key=Name,Value=TradingBot}]' \
  --block-device-mappings '[{"DeviceName":"/dev/sda1","Ebs":{"VolumeSize":100}}]'
```

Retrieve the public IP:
```bash
aws ec2 describe-instances \
  --filters "Name=tag:Name,Values=TradingBot" \
  --query 'Reservations[*].Instances[*].PublicIpAddress' \
  --output text
```

## 5. Server Bootstrap
```bash
ssh -i ~/trading-bot-key.pem ubuntu@YOUR_INSTANCE_IP
sudo apt update && sudo apt upgrade -y
curl -fsSL https://get.docker.com -o get-docker.sh
sudo sh get-docker.sh
sudo usermod -aG docker ubuntu
sudo curl -L "https://github.com/docker/compose/releases/latest/download/docker-compose-$(uname -s)-$(uname -m)" -o /usr/local/bin/docker-compose
sudo chmod +x /usr/local/bin/docker-compose
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env
sudo mkdir -p /opt/trading-bot
sudo chown ubuntu:ubuntu /opt/trading-bot
exit
```

Log back in to confirm Docker group membership before deployments.
