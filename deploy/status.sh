#!/bin/bash
# ============================================================================
# GTFS Validator - Status Check Script
# Run this ON THE SERVER to check the current status
# Usage: ./deploy/status.sh
# ============================================================================

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo ""
echo "📊 GTFS Validator - Status"
echo "=========================="
echo ""

# Check Docker
echo -e "${BLUE}Docker Status:${NC}"
if systemctl is-active --quiet docker; then
    echo -e "  Docker: ${GREEN}running${NC}"
else
    echo -e "  Docker: ${RED}stopped${NC}"
fi

echo ""

# Check containers
echo -e "${BLUE}Containers:${NC}"
docker compose ps 2>/dev/null || echo "  (docker compose not available in this directory)"

echo ""

# Check health endpoint
echo -e "${BLUE}Health Check:${NC}"
if curl -sf http://localhost:3000/healthz &>/dev/null; then
    echo -e "  API: ${GREEN}healthy${NC}"
    
    # Get version if available
    VERSION=$(curl -sf http://localhost:3000/version 2>/dev/null || echo '{"version":"unknown"}')
    echo "  Version: $VERSION"
else
    echo -e "  API: ${RED}not responding${NC}"
fi

echo ""

# Check disk usage
echo -e "${BLUE}Disk Usage:${NC}"
df -h / | tail -1 | awk '{print "  Root: " $5 " used (" $3 " of " $2 ")"}'

# Check memory
echo ""
echo -e "${BLUE}Memory Usage:${NC}"
free -h | grep Mem | awk '{print "  RAM: " $3 " used of " $2}'

# Docker volume size
echo ""
echo -e "${BLUE}Docker Volumes:${NC}"
docker system df -v 2>/dev/null | grep -A 100 "VOLUME NAME" | head -10 || echo "  (unable to check)"

echo ""
