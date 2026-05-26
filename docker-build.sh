#!/usr/bin/env bash
set -euo pipefail

ACTION=build
BOARD=c3
PORT="${ESP_PORT:-/dev/ttyACM0}"

usage() {
    cat <<EOF
Usage: $0 [--c3|--wroom32] [--flash|--monitor]

  --c3        Build for ESP32-C3 (default)
  --wroom32   Build for ESP-WROOM-32
  --flash     Build, flash, and monitor using \$ESP_PORT (default: /dev/ttyACM0)
  --monitor   Monitor only using \$ESP_PORT (default: /dev/ttyACM0)
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --c3)
            BOARD=c3
            ;;
        --wroom32)
            BOARD=wroom32
            ;;
        --flash)
            [[ "$ACTION" == build ]] || { usage >&2; exit 2; }
            ACTION=flash
            ;;
        --monitor)
            [[ "$ACTION" == build ]] || { usage >&2; exit 2; }
            ACTION=monitor
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
    shift
done

# Load WiFi credentials and other env vars
if [[ -f env.sh ]]; then
    # shellcheck source=env.sh
    source env.sh
fi

case "$BOARD" in
    c3)
        ESP_BOARD=esp32c3
        MCU=esp32c3
        BIN=target/riscv32imc-esp-espidf/release/esp32multical21
        BUILD_COMMAND=(cargo build -r)
        ;;
    wroom32)
        ESP_BOARD=esp32
        MCU=esp32
        BIN=target/xtensa-esp32-espidf/release/esp32multical21
        BUILD_COMMAND=(cargo +esp build -r --target xtensa-esp32-espidf --no-default-features --features=esp-wroom-32)
        ;;
esac

# docker run -e NAME forwards variables from this process only when they are
# exported. env.sh is commonly a plain shell assignment file.
export MCU WIFI_SSID WIFI_PASS
IMAGE="esp32multical21-builder-$ESP_BOARD"

# Build image if not present
if ! docker image inspect "$IMAGE" &>/dev/null; then
    docker build --build-arg "ESP_BOARD=$ESP_BOARD" -t "$IMAGE" .
fi

require_port() {
    if [[ ! -e "$PORT" ]]; then
        echo "Device $PORT not found. Set ESP_PORT to the serial device, for example /dev/ttyUSB0." >&2
        exit 1
    fi
}

# Monitor only - no build
if [[ "$ACTION" == monitor ]]; then
    require_port
    exec docker run --rm -it \
        -v "$(pwd)":/project \
        --device "$PORT" \
        --group-add "$(stat -c '%g' "$PORT")" \
        "$IMAGE" \
        espflash monitor --port "$PORT"
fi

# Ensure the Cargo cache volume is writable by the container user (idempotent)
docker run --rm --user root \
    -v esp32-cargo-cache:/cache/cargo \
    "$IMAGE" \
    chown esp:esp /cache/cargo

# The custom Xtensa compiler in the ESP32 image does not ship Clippy.
if [[ "$BOARD" == c3 ]]; then
    docker run --rm \
        -e MCU -e WIFI_SSID -e WIFI_PASS \
        -v "$(pwd)":/project \
        -v esp32-cargo-cache:/cache/cargo \
        "$IMAGE" \
        cargo clippy --all-targets --all-features -- -D warnings
fi

# Build
docker run --rm \
    -e MCU -e WIFI_SSID -e WIFI_PASS \
    -v "$(pwd)":/project \
    -v esp32-cargo-cache:/cache/cargo \
    "$IMAGE" \
    "${BUILD_COMMAND[@]}"

echo ""
echo "Build done for $BOARD: $BIN ($(du -h "$BIN" | cut -f1))"

# Flash
if [[ "$ACTION" != flash ]]; then
    exit 0
fi
require_port

echo "Flashing to $PORT ..."
docker run --rm -it \
    -v "$(pwd)":/project \
    --device "$PORT" \
    --group-add "$(stat -c '%g' "$PORT")" \
    "$IMAGE" \
    espflash flash --monitor \
        --partition-table ./partitions.csv \
        --erase-parts otadata \
        --baud 921600 \
        --port "$PORT" \
        "$BIN"
