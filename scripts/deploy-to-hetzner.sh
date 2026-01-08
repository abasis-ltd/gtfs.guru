#!/bin/bash
# ============================================================================
# Deploy GTFS Validator to Hetzner from local machine
# Usage: ./scripts/deploy-to-hetzner.sh [server-ip-or-hostname]
# ============================================================================

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step() { echo -e "${BLUE}[STEP]${NC} $1"; }

# Get server from argument or environment
SERVER="${1:-${HETZNER_SERVER:-}}"

if [[ -z "$SERVER" ]]; then
    echo "Usage: $0 <server-ip-or-hostname>"
    echo ""
    echo "Examples:"
    echo "  $0 123.45.67.89"
    echo "  $0 gtfs.example.com"
    echo "  $0 root@123.45.67.89"
    echo ""
    echo "Or set HETZNER_SERVER environment variable:"
    echo "  export HETZNER_SERVER=123.45.67.89"
    exit 1
fi

# Add root@ if not specified
if [[ "$SERVER" != *"@"* ]]; then
    SERVER="root@$SERVER"
fi

REMOTE_DIR="~/gtfs-guru-web"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo ""
echo "🚀 GTFS Validator - Deploy to Hetzner"
echo "======================================"
echo "Server: $SERVER"
echo "Remote directory: $REMOTE_DIR"
echo ""

# ============================================================================
# Step 1: Check SSH connection
# ============================================================================
log_step "Checking SSH connection..."
if ! ssh -o ConnectTimeout=5 "$SERVER" "echo 'SSH OK'" &>/dev/null; then
    log_error "Cannot connect to $SERVER"
    log_error "Make sure SSH key is configured and server is accessible"
    exit 1
fi
log_info "SSH connection OK"

# ============================================================================
# Step 2: Check Docker on remote
# ============================================================================
log_step "Checking Docker on remote server..."
if ! ssh "$SERVER" "docker --version" &>/dev/null; then
    log_warn "Docker not found. Running initial setup..."
    ssh "$SERVER" "curl -fsSL https://get.docker.com | sh && systemctl enable docker && systemctl start docker"
fi
log_info "Docker is available"

# ============================================================================
# Step 3: Create remote directory
# ============================================================================
log_step "Creating remote directory..."
ssh "$SERVER" "mkdir -p $REMOTE_DIR"

# ============================================================================
# Step 4: Sync project files
# ============================================================================
log_step "Syncing project files to server..."
rsync -avz --progress \
    --exclude 'target/' \
    --exclude '.git/' \
    --exclude 'test-gtfs-feeds/' \
    --exclude '*.log' \
    --exclude '.DS_Store' \
    --exclude 'node_modules/' \
    --exclude '.env' \
    "$PROJECT_DIR/" "$SERVER:$REMOTE_DIR/"

log_info "Files synced successfully"

# ============================================================================
# Step 5: Build and start on remote
# ============================================================================
log_step "Building Docker image on server..."
ssh "$SERVER" "cd $REMOTE_DIR && docker compose build"

log_step "Starting services..."
ssh "$SERVER" "cd $REMOTE_DIR && docker compose up -d"

# ============================================================================
# Step 6: Check health
# ============================================================================
log_step "Waiting for service to start..."
sleep 5

if ssh "$SERVER" "curl -sf http://localhost:3000/healthz" &>/dev/null; then
    log_info "Service is healthy!"
else
    log_warn "Service may still be starting. Check with: ssh $SERVER 'docker compose logs -f'"
fi

# ============================================================================
# Done
# ============================================================================
echo ""
echo "=============================================="
echo -e "${GREEN}✅ Deployment complete!${NC}"
echo "=============================================="
echo ""
echo "📋 Next steps:"
echo "   1. Configure your domain in Caddyfile on the server"
echo "   2. Start with HTTPS: docker compose --profile https up -d"
echo ""
echo "🔧 Useful commands:"
echo "   ssh $SERVER 'cd $REMOTE_DIR && docker compose logs -f'"
echo "   ssh $SERVER 'cd $REMOTE_DIR && docker compose restart'"
echo ""
