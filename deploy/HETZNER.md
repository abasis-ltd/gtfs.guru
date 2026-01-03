# 🚀 Hetzner Deployment Guide

Deploy the GTFS Validator web service to Hetzner Cloud in minutes.

## Prerequisites

- Hetzner Cloud account
- Domain name (optional, but recommended for HTTPS)
- SSH key for server access

## Quick Start

### 1. Create a Hetzner Server

1. Go to [Hetzner Cloud Console](https://console.hetzner.cloud/)
2. Create a new project or select existing
3. Add a server with these settings:
   - **Location**: Choose closest to your users
   - **Image**: Ubuntu 24.04 (or 22.04)
   - **Type**: CX22 (2 vCPU, 4GB RAM) - recommended minimum
   - **SSH Key**: Add your SSH key
   - **Name**: `gtfs-validator`

### 2. Point Your Domain (Optional)

Add an A record pointing to your server's IP:
```
gtfs.yourdomain.com  →  YOUR_SERVER_IP
```

### 3. Deploy

**Option A: Deploy from your local machine (recommended)**

```bash
# From the project root directory
./scripts/deploy-to-hetzner.sh YOUR_SERVER_IP
```

**Option B: SSH into server and run setup script**

```bash
ssh root@YOUR_SERVER_IP
curl -fsSL https://get.docker.com | sh
git clone https://github.com/YOUR_ORG/gtfs-validator-rust.git /opt/gtfs-validator
cd /opt/gtfs-validator
docker compose build
docker compose up -d
```

## Manual Deployment

### Step 1: Install Docker

```bash
curl -fsSL https://get.docker.com | sh
sudo systemctl enable docker
sudo systemctl start docker
```

### Step 2: Clone Repository

```bash
cd /opt
git clone https://github.com/YOUR_ORG/gtfs-validator-rust.git gtfs-validator
cd gtfs-validator
```

### Step 3: Configure

Edit the Caddyfile with your domain:
```bash
nano Caddyfile
# Replace gtfs.yourdomain.com with your actual domain
```

Create `.env` file:
```bash
cp .env.example .env
nano .env
# Set GTFS_VALIDATOR_WEB_PUBLIC_BASE_URL=https://yourdomain.com
```

### Step 4: Start Services

```bash
# Without HTTPS (development)
docker compose up -d

# With HTTPS via Caddy
docker compose --profile https up -d
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `GTFS_VALIDATOR_WEB_PUBLIC_BASE_URL` | `http://localhost:3000` | Public URL for the service |
| `GTFS_VALIDATOR_WEB_BASE_DIR` | `/data/jobs` | Directory for job storage |
| `GTFS_VALIDATOR_WEB_JOB_TTL_SECONDS` | `86400` | Job retention (24 hours) |
| `RUST_LOG` | `info` | Log level |

## Management Commands

```bash
# View logs
docker compose logs -f gtfs-validator

# Restart service
docker compose restart

# Update to latest version
./deploy/update.sh

# Check service health
curl http://localhost:3000/healthz
```

## Deployment Scripts

The `deploy/` directory contains helper scripts for managing your deployment:

| Script | Description |
|--------|-------------|
| `deploy/update.sh` | Update to latest version with zero downtime |
| `deploy/status.sh` | Check service status, health, and resource usage |
| `deploy/backup.sh` | Backup job data from Docker volumes |
| `deploy/hetzner-setup.sh` | Initial server setup (run once) |

From your local machine:
```bash
# Deploy from local machine to server
./scripts/deploy-to-hetzner.sh YOUR_SERVER_IP
```

## Resource Recommendations

| Server Type | RAM | vCPU | Concurrent Jobs | Monthly Cost |
|-------------|-----|------|-----------------|--------------|
| CX22 | 4 GB | 2 | ~5-10 | ~€4 |
| CX32 | 8 GB | 4 | ~10-20 | ~€8 |
| CX42 | 16 GB | 8 | ~20-50 | ~€16 |

## Monitoring

### Health Check Endpoint
```bash
curl https://yourdomain.com/healthz
# Returns: ok
```

### Version Check
```bash
curl https://yourdomain.com/version
# Returns: {"version":"0.1.0"}
```

## Backup

Job data is stored in Docker volume `gtfs-data`. To backup:

```bash
# Create backup
docker run --rm -v gtfs-data:/data -v $(pwd):/backup alpine \
    tar czf /backup/gtfs-backup-$(date +%Y%m%d).tar.gz /data

# Restore backup
docker run --rm -v gtfs-data:/data -v $(pwd):/backup alpine \
    tar xzf /backup/gtfs-backup-YYYYMMDD.tar.gz -C /
```

## Troubleshooting

### Container won't start
```bash
docker compose logs gtfs-validator
```

### SSL certificate issues
```bash
# Check Caddy logs
docker compose logs caddy

# Ensure DNS is pointing to server
dig +short yourdomain.com
```

### Out of memory
Increase the memory limit in `docker-compose.yml`:
```yaml
deploy:
  resources:
    limits:
      memory: 4G  # Increase as needed
```

## Security Considerations

- ✅ Non-root container user
- ✅ Health checks enabled
- ✅ Automatic HTTPS with Caddy
- ✅ Security headers configured
- ✅ Resource limits set
- ✅ Log rotation enabled

## Updating

```bash
cd /opt/gtfs-validator
git pull
docker compose build --no-cache
docker compose up -d
```
