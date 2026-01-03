#!/bin/bash
# ============================================================================
# GTFS Validator - Hetzner Server Setup Script
# Run this on a fresh Ubuntu 22.04/24.04 server
# Usage: curl -sSL https://raw.githubusercontent.com/YOUR_REPO/deploy/hetzner-setup.sh | bash
# ============================================================================

set -euo pipefail

echo "🚀 GTFS Validator - Hetzner Setup"
echo "================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   log_error "This script must be run as root"
   exit 1
fi

# ============================================================================
# 1. System Update
# ============================================================================
log_info "Updating system packages..."
apt-get update && apt-get upgrade -y

# ============================================================================
# 2. Install Docker
# ============================================================================
log_info "Installing Docker..."
if ! command -v docker &> /dev/null; then
    curl -fsSL https://get.docker.com | sh
    systemctl enable docker
    systemctl start docker
    log_info "Docker installed successfully"
else
    log_info "Docker already installed"
fi

# ============================================================================
# 3. Install Docker Compose
# ============================================================================
log_info "Checking Docker Compose..."
if ! docker compose version &> /dev/null; then
    apt-get install -y docker-compose-plugin
    log_info "Docker Compose plugin installed"
else
    log_info "Docker Compose already available"
fi

# ============================================================================
# 4. Configure Firewall
# ============================================================================
log_info "Configuring firewall..."
apt-get install -y ufw

# Allow SSH
ufw allow 22/tcp

# Allow HTTP/HTTPS
ufw allow 80/tcp
ufw allow 443/tcp

# Enable firewall (non-interactive)
echo "y" | ufw enable

log_info "Firewall configured"

# ============================================================================
# 5. Create Application Directory
# ============================================================================
APP_DIR="/opt/gtfs-validator"
log_info "Creating application directory: $APP_DIR"
mkdir -p "$APP_DIR"
cd "$APP_DIR"

# ============================================================================
# 6. Create docker-compose.yml
# ============================================================================
log_info "Creating docker-compose.yml..."
cat > docker-compose.yml << 'DOCKER_COMPOSE'
services:
  gtfs-validator:
    image: ghcr.io/your-org/gtfs-validator-web:latest
    container_name: gtfs-validator
    restart: unless-stopped
    environment:
      - GTFS_VALIDATOR_WEB_BASE_DIR=/data/jobs
      - GTFS_VALIDATOR_WEB_PUBLIC_BASE_URL=${PUBLIC_URL:-http://localhost:3000}
      - GTFS_VALIDATOR_WEB_JOB_TTL_SECONDS=86400
      - RUST_LOG=info
    volumes:
      - gtfs-data:/data/jobs
    deploy:
      resources:
        limits:
          memory: 2G
    healthcheck:
      test: ["CMD", "curl", "-sf", "http://localhost:3000/healthz"]
      interval: 30s
      timeout: 3s
      retries: 3
    logging:
      driver: "json-file"
      options:
        max-size: "10m"
        max-file: "3"

  caddy:
    image: caddy:2-alpine
    container_name: caddy
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - caddy-data:/data
      - caddy-config:/config
    depends_on:
      - gtfs-validator

volumes:
  gtfs-data:
  caddy-data:
  caddy-config:
DOCKER_COMPOSE

# ============================================================================
# 7. Create Caddyfile (prompt for domain)
# ============================================================================
echo ""
read -p "Enter your domain (e.g., gtfs.example.com): " DOMAIN
DOMAIN=${DOMAIN:-localhost}

log_info "Configuring Caddy for domain: $DOMAIN"
cat > Caddyfile << CADDYFILE
$DOMAIN {
    reverse_proxy gtfs-validator:3000
    encode gzip zstd
    
    header {
        Strict-Transport-Security "max-age=31536000; includeSubDomains"
        X-Frame-Options "SAMEORIGIN"
        X-Content-Type-Options "nosniff"
        -Server
    }
    
    request_body {
        max_size 500MB
    }
}
CADDYFILE

# ============================================================================
# 8. Create .env file
# ============================================================================
log_info "Creating .env file..."
cat > .env << ENV
PUBLIC_URL=https://$DOMAIN
ENV

# ============================================================================
# 9. Create systemd service
# ============================================================================
log_info "Creating systemd service..."
cat > /etc/systemd/system/gtfs-validator.service << 'SYSTEMD'
[Unit]
Description=GTFS Validator Web Service
Requires=docker.service
After=docker.service

[Service]
Type=oneshot
RemainAfterExit=yes
WorkingDirectory=/opt/gtfs-validator
ExecStart=/usr/bin/docker compose up -d
ExecStop=/usr/bin/docker compose down
TimeoutStartSec=0

[Install]
WantedBy=multi-user.target
SYSTEMD

systemctl daemon-reload
systemctl enable gtfs-validator

# ============================================================================
# 10. Start Services
# ============================================================================
log_info "Starting GTFS Validator..."
docker compose up -d

# ============================================================================
# Done!
# ============================================================================
echo ""
echo "=============================================="
echo -e "${GREEN}✅ GTFS Validator deployed successfully!${NC}"
echo "=============================================="
echo ""
echo "📌 Access your validator at: https://$DOMAIN"
echo ""
echo "📋 Useful commands:"
echo "   docker compose logs -f        # View logs"
echo "   docker compose restart        # Restart services"
echo "   docker compose pull && docker compose up -d  # Update"
echo ""
echo "🔒 SSL certificate will be automatically provisioned by Caddy"
echo "   (make sure your domain's DNS points to this server!)"
echo ""
