#!/bin/bash
# ============================================================================
# GTFS Validator - Update Script
# Run this ON THE SERVER to update to the latest version
# Usage: cd /opt/gtfs-validator && ./deploy/update.sh
# ============================================================================

set -euo pipefail

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }

echo ""
echo "🔄 GTFS Validator - Update"
echo "=========================="
echo ""

# Check if we're in the right directory
if [[ ! -f "docker-compose.yml" ]]; then
    echo "Error: docker-compose.yml not found"
    echo "Please run this script from /opt/gtfs-validator"
    exit 1
fi

# Pull latest changes if this is a git repo
if [[ -d ".git" ]]; then
    log_info "Pulling latest changes from git..."
    git pull --ff-only
fi

# Rebuild the image
log_info "Rebuilding Docker image..."
docker compose build --no-cache

# Restart services with zero downtime
log_info "Restarting services..."
docker compose up -d

# Cleanup old images
log_info "Cleaning up old Docker images..."
docker image prune -f

# Health check
log_info "Checking service health..."
sleep 3

if curl -sf http://localhost:3000/healthz &>/dev/null; then
    echo ""
    echo -e "${GREEN}✅ Update complete! Service is healthy.${NC}"
else
    log_warn "Service may still be starting..."
    echo "Check logs with: docker compose logs -f"
fi

echo ""
