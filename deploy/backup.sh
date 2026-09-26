#!/bin/bash
# ============================================================================
# GTFS Validator - Backup Script
# Run this ON THE SERVER to create a backup of job data
# Usage: ./deploy/backup.sh [backup-directory]
#
# GTFS_BACKUP_VOLUME names the Docker volume holding /data/jobs. The default is
# what `docker compose` names it from /opt/gtfs-validator; a container started
# with plain `docker run` (production) may use another name -- check with
# `docker inspect gtfs-validator --format '{{json .Mounts}}'`.
# ============================================================================

set -euo pipefail

BACKUP_DIR="${1:-/root/backups}"
VOLUME="${GTFS_BACKUP_VOLUME:-gtfs-validator_gtfs-data}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="gtfs-validator-backup-$TIMESTAMP.tar.gz"

echo ""
echo "💾 GTFS Validator - Backup"
echo "=========================="
echo ""

# `docker run -v name:/x` silently creates a missing volume, so a wrong name
# would produce an empty archive and a "Backup complete!".
if ! docker volume inspect "$VOLUME" >/dev/null 2>&1; then
    echo "❌ Docker volume '$VOLUME' does not exist; nothing was backed up." >&2
    echo "   Set GTFS_BACKUP_VOLUME to one of these:" >&2
    docker volume ls --format '   {{.Name}}' >&2 || true
    exit 1
fi

# Create backup directory
mkdir -p "$BACKUP_DIR"

echo "Creating backup: $BACKUP_DIR/$BACKUP_FILE"

# Backup Docker volume data
docker run --rm \
    -v "$VOLUME":/data:ro \
    -v "$BACKUP_DIR":/backup \
    alpine \
    tar czf "/backup/$BACKUP_FILE" -C /data .

# Show backup size
BACKUP_SIZE=$(du -h "$BACKUP_DIR/$BACKUP_FILE" | cut -f1)
echo ""
echo "✅ Backup complete!"
echo "   File: $BACKUP_DIR/$BACKUP_FILE"
echo "   Size: $BACKUP_SIZE"
echo ""

# Cleanup old backups (keep last 7)
echo "Cleaning up old backups (keeping last 7)..."
ls -t "$BACKUP_DIR"/gtfs-validator-backup-*.tar.gz 2>/dev/null | tail -n +8 | xargs -r rm -f

echo "Done!"
echo ""
