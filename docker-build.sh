#!/bin/bash
set -e

IMAGE=esp32multical21-builder
BIN=target/riscv32imc-esp-espidf/release/esp32multical21
PORT=/dev/ttyACM0

# Load WiFi credentials and other env vars
if [[ -f env.sh ]]; then
    # shellcheck source=env.sh
    source env.sh
fi

# Build image if not present
if ! docker image inspect "$IMAGE" &>/dev/null; then
    docker build -t "$IMAGE" .
fi

# Monitor only — no build
if [[ "$1" == "--monitor" ]]; then
    exec docker run --rm -it \
        -v "$(pwd)":/project \
        --device "$PORT" \
        --group-add "$(getent group dialout | cut -d: -f3)" \
        "$IMAGE" \
        espflash monitor --port "$PORT"
fi

# Ensure cache volumes are writable by the container user (idempotent)
docker run --rm --user root \
    -v esp32-espressif-cache:/cache/espressif \
    -v esp32-cargo-cache:/cache/cargo \
    "$IMAGE" \
    chown esp:esp /cache/espressif /cache/cargo

# Clippy
docker run --rm \
    -e WIFI_SSID -e WIFI_PASS \
    -v "$(pwd)":/project \
    -v esp32-espressif-cache:/cache/espressif \
    -v esp32-cargo-cache:/cache/cargo \
    "$IMAGE" \
    cargo clippy --all-targets --all-features -- -D warnings

# Build
docker run --rm \
    -e WIFI_SSID -e WIFI_PASS \
    -v "$(pwd)":/project \
    -v esp32-espressif-cache:/cache/espressif \
    -v esp32-cargo-cache:/cache/cargo \
    "$IMAGE"

echo ""
echo "Build done: $BIN ($(du -h $BIN | cut -f1))"

# Flash
if [[ "$1" != "--flash" ]]; then
    exit 0
fi
if [[ ! -e "$PORT" ]]; then
    echo "Device $PORT not found, cannot flash."
    exit 1
fi

echo "Flashing to $PORT ..."
docker run --rm -it \
    -v "$(pwd)":/project \
    --device "$PORT" \
    --group-add "$(getent group dialout | cut -d: -f3)" \
    "$IMAGE" \
    espflash flash --monitor \
        --partition-table ./partitions.csv \
        --erase-parts otadata \
        --baud 921600 \
        --port "$PORT" \
        "$BIN"
