#!/bin/bash
set -e

IMAGE=esp32multical21-builder
BIN=target/riscv32imc-esp-espidf/release/esp32multical21
PORT=/dev/ttyACM0

# Build
docker run --rm \
    -v "$(pwd)":/project \
    -v esp32-espressif-cache:/cache/espressif \
    -v esp32-cargo-cache:/cache/cargo \
    "$IMAGE"

echo ""
echo "Build done: $BIN ($(du -h $BIN | cut -f1))"

# Flash (skip if --no-flash or device not present)
if [[ "$1" == "--no-flash" ]]; then
    exit 0
fi
if [[ ! -e "$PORT" ]]; then
    echo "Device $PORT not found, skipping flash."
    exit 0
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
