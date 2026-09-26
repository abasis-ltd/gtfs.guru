#!/usr/bin/env bash
# ============================================================================
# Replace the running gtfs-guru-web container with a freshly loaded image.
#
# Runs ON THE SERVER, fed over ssh by .github/workflows/deploy-web.yml:
#
#   ssh host 'bash -s -- <image>:<tag> <commit>' < deploy/swap-web-container.sh
#
# Production does not run from docker-compose.yml: Caddy on the host proxies
# gtfs.guru to a container that was started with plain `docker run`. So instead
# of hard-coding ports, env and volumes here (and silently overwriting whatever
# was tuned on the box), the script reads them off the container it replaces
# and starts the new one with the same settings. If the new container is not
# healthy within the deadline, the old image is put back.
# ============================================================================
set -euo pipefail

IMAGE="${1:?usage: swap-web-container.sh <image:tag> <commit>}"
COMMIT="${2:?usage: swap-web-container.sh <image:tag> <commit>}"
NAME="${GTFS_WEB_CONTAINER:-gtfs-validator}"
HEALTH_DEADLINE="${GTFS_WEB_HEALTH_DEADLINE:-90}"

log() { printf '[swap] %s\n' "$*"; }

docker image inspect "$IMAGE" >/dev/null 2>&1 || {
    log "image $IMAGE is not loaded on this host"
    exit 1
}

if ! docker container inspect "$NAME" >/dev/null 2>&1; then
    log "no container named $NAME to replace; refusing to guess its ports and env"
    exit 1
fi

# --- Read the live container's settings ------------------------------------
previous_image=$(docker inspect "$NAME" --format '{{.Config.Image}}')
restart_policy=$(docker inspect "$NAME" --format '{{.HostConfig.RestartPolicy.Name}}')
memory_limit=$(docker inspect "$NAME" --format '{{.HostConfig.Memory}}')

run_args=(--detach --name "$NAME" --restart "${restart_policy:-always}")
if [ "${memory_limit:-0}" != "0" ]; then
    run_args+=(--memory "$memory_limit")
fi

# Ports: "hostIp:hostPort:containerPort/proto" per published binding. Kept in
# their own list so the health-port lookup below never sees --env values.
publish_bindings=()
while IFS= read -r binding; do
    [ -n "$binding" ] || continue
    publish_bindings+=("$binding")
    run_args+=(--publish "$binding")
done < <(docker inspect "$NAME" --format \
    '{{range $port, $bindings := .HostConfig.PortBindings}}{{range $bindings}}{{if .HostIp}}{{.HostIp}}:{{end}}{{.HostPort}}:{{$port}}{{"\n"}}{{end}}{{end}}')

# Runtime and hardening options, so a swap does not quietly drop log rotation,
# the non-root user or the capability/seccomp settings tuned on the box.
# Not carried over: extra networks beyond the primary one, network aliases,
# tmpfs, devices, swap/CPU-set tuning and labels. Add them here if the live
# container starts using them.
log_driver=$(docker inspect "$NAME" --format '{{.HostConfig.LogConfig.Type}}')
if [ -n "$log_driver" ]; then
    run_args+=(--log-driver "$log_driver")
fi
while IFS= read -r opt; do
    [ -n "$opt" ] || continue
    run_args+=(--log-opt "$opt")
done < <(docker inspect "$NAME" --format \
    '{{range $key, $value := .HostConfig.LogConfig.Config}}{{$key}}={{$value}}{{"\n"}}{{end}}')

user=$(docker inspect "$NAME" --format '{{.Config.User}}')
if [ -n "$user" ]; then
    run_args+=(--user "$user")
fi

network_mode=$(docker inspect "$NAME" --format '{{.HostConfig.NetworkMode}}')
case "$network_mode" in
    ""|default|bridge) ;;
    *) run_args+=(--network "$network_mode") ;;
esac

while IFS= read -r cap; do
    [ -n "$cap" ] || continue
    run_args+=(--cap-drop "$cap")
done < <(docker inspect "$NAME" --format '{{range .HostConfig.CapDrop}}{{.}}{{"\n"}}{{end}}')
while IFS= read -r cap; do
    [ -n "$cap" ] || continue
    run_args+=(--cap-add "$cap")
done < <(docker inspect "$NAME" --format '{{range .HostConfig.CapAdd}}{{.}}{{"\n"}}{{end}}')
while IFS= read -r opt; do
    [ -n "$opt" ] || continue
    run_args+=(--security-opt "$opt")
done < <(docker inspect "$NAME" --format '{{range .HostConfig.SecurityOpt}}{{.}}{{"\n"}}{{end}}')
while IFS= read -r limit; do
    [ -n "$limit" ] || continue
    run_args+=(--ulimit "$limit")
done < <(docker inspect "$NAME" --format '{{range .HostConfig.Ulimits}}{{.Name}}={{.Soft}}:{{.Hard}}{{"\n"}}{{end}}')

if [ "$(docker inspect "$NAME" --format '{{.HostConfig.ReadonlyRootfs}}')" = "true" ]; then
    run_args+=(--read-only)
fi
nano_cpus=$(docker inspect "$NAME" --format '{{.HostConfig.NanoCpus}}')
if [ "${nano_cpus:-0}" != "0" ]; then
    run_args+=(--cpus "$(awk -v n="$nano_cpus" 'BEGIN { printf "%.3f", n / 1e9 }')")
fi
pids_limit=$(docker inspect "$NAME" --format '{{if .HostConfig.PidsLimit}}{{.HostConfig.PidsLimit}}{{end}}')
if [ -n "$pids_limit" ] && [ "$pids_limit" -gt 0 ] 2>/dev/null; then
    run_args+=(--pids-limit "$pids_limit")
fi

# Named volumes and bind mounts.
while IFS= read -r mount; do
    [ -n "$mount" ] || continue
    run_args+=(--volume "$mount")
done < <(docker inspect "$NAME" --format \
    '{{range .Mounts}}{{if eq .Type "volume"}}{{.Name}}{{else}}{{.Source}}{{end}}:{{.Destination}}{{if not .RW}}:ro{{end}}{{"\n"}}{{end}}')

# Environment, minus PATH (the image sets its own) and minus the build commit
# (the new image carries its own value).
while IFS= read -r kv; do
    [ -n "$kv" ] || continue
    case "$kv" in
        PATH=*|GTFS_GURU_BUILD_COMMIT=*) continue ;;
    esac
    run_args+=(--env "$kv")
done < <(docker inspect "$NAME" --format '{{range .Config.Env}}{{.}}{{"\n"}}{{end}}')

# --- Swap ------------------------------------------------------------------
health_url=""
for binding in ${publish_bindings[@]+"${publish_bindings[@]}"}; do
    case "$binding" in
        *:3000/tcp)
            host_part="${binding%:3000/tcp}"   # "[hostIp:]hostPort"
            host_port="${host_part##*:}"
            host_ip="127.0.0.1"
            if [ "$host_part" != "$host_port" ]; then
                bound_ip="${host_part%:*}"
                # A wildcard binding answers on loopback; a specific IPv4 one
                # may only answer on that address.
                case "$bound_ip" in
                    ""|0.0.0.0|::|*:*) ;;
                    *) host_ip="$bound_ip" ;;
                esac
            fi
            health_url="http://${host_ip}:${host_port}"
            ;;
    esac
done
if [ -z "$health_url" ]; then
    log "could not find the published port for 3000/tcp; aborting before touching $NAME"
    exit 1
fi

wait_healthy() {
    local expected="$1" deadline="$2" reported
    for _ in $(seq 1 "$deadline"); do
        if reported=$(curl -sf --max-time 2 "$health_url/version" 2>/dev/null); then
            case "$reported" in
                *"\"commit\":\"$expected\""*) return 0 ;;
            esac
        fi
        sleep 1
    done
    return 1
}

log "replacing $NAME ($previous_image) with $IMAGE (commit $COMMIT)"
docker rename "$NAME" "$NAME-previous"
docker stop --time 20 "$NAME-previous" >/dev/null

if docker run "${run_args[@]}" "$IMAGE" >/dev/null && wait_healthy "$COMMIT" "$HEALTH_DEADLINE"; then
    docker rm "$NAME-previous" >/dev/null
    log "healthy: $(curl -sf "$health_url/version")"
else
    log "new container did not report commit $COMMIT within ${HEALTH_DEADLINE}s; rolling back"
    docker logs --tail 50 "$NAME" 2>&1 | sed 's/^/[new] /' || true
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    docker rename "$NAME-previous" "$NAME"
    docker start "$NAME" >/dev/null
    exit 1
fi

# Keep the previous image for a manual rollback; drop anything older.
docker tag "$IMAGE" "${IMAGE%%:*}:latest"
docker images "${IMAGE%%:*}" --format '{{.Repository}}:{{.Tag}}' \
    | grep -vE ":(latest|${IMAGE##*:}|${previous_image##*:})$" \
    | xargs -r docker rmi >/dev/null 2>&1 || true
log "done"
