# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ESP32 Multical21 — a Rust embedded IoT device for ESP32-C3 (default) or ESP32-S2. It reads wireless M-Bus meter data via a CC1101 868 MHz RF module and exposes a web configuration UI, REST API, and MQTT publisher. Runs on ESP-IDF with Tokio async runtime.

## Build & Flash Commands

```bash
# Source environment variables (WiFi credentials, API port, MCU selection)
source env.sh

# Build (debug)
cargo build

# Build release and flash to device at 921600 baud (shortcut: ./flash)
cargo run -r -- --baud 921600

# Build only (release)
cargo build -r

# Clippy lint
cargo clippy

# Target a different MCU (default is esp32c3)
cargo build --features esp32s --no-default-features
```

The build target is `riscv32imc-esp-espidf` (configured in `.cargo/config.toml`). The toolchain is nightly with `rust-src` for custom std builds against ESP-IDF v5.4.3. Flash uses dual OTA partition table (`partitions.csv`) with `--erase-parts otadata` to reset OTA tracking on fresh flash.

## Architecture

The binary entry point is `src/bin/esp32multical21.rs`. It initializes hardware (SPI for CC1101 RF module, GPIO for reset button), loads config from NVS, sets up WiFi, then runs five concurrent tasks via `tokio::select!`:

| Task | Module | Purpose |
|------|--------|---------|
| `poll_reset()` | bin | 2s heartbeat, uptime tracking, factory reset on 4.5s button hold |
| `poll_sensors()` | `measure.rs` | CC1101 radio reception, wMBus decoding, meter data parsing |
| `run_mqtt()` | `mqtt.rs` | MQTT client, publishes to `{topic}/uptime` and `{topic}/meter` |
| `run_api_server()` | `apiserver.rs` | Axum HTTP server (port 80 default) |
| `wifi_loop.run()` | `wifi.rs` | WiFi connection/reconnection manager |

**Shared state** (`state.rs`): `MyState` struct with Tokio `RwLock` fields (config, uptime, wifi status, IP, etc.) wrapped in `Arc` and passed to all tasks.

**Configuration** (`config.rs`): `MyConfig` struct serialized with Postcard + CRC-32 into ESP-IDF NVS. Covers WiFi, static IP/DHCP, MQTT, polling parameters, meter ID and AES key. Defaults can be overridden via env vars (`WIFI_SSID`, `WIFI_PASS`, `API_PORT`).

**Web UI**: Askama template (`templates/index.html.ask`) renders a config form. Static assets (`form.js`, `favicon.ico`) are embedded in the binary from `src/`.

**API routes** (Axum, in `apiserver.rs`):
- `GET /` — config page, `GET /uptime` — JSON uptime, `GET /conf` — JSON config
- `POST /conf` — save config & reboot, `GET /reset_conf` — factory reset
- `GET /meter` — JSON current meter reading
- `POST /fw` — OTA firmware update (accepts form field `url` pointing to firmware binary)

## Key Conventions

- Sentinel value for no temperature reading: `NO_TEMP = -1000.0` (defined in `lib.rs`)
- Device ID derived from MAC address: `esp32temp-XX:XX:XX:XX:XX:XX`
- MQTT client ID follows same pattern as device ID
- Release profile uses `opt-level = "z"` (size) with fat LTO
- Clippy `future-size-threshold = 128` (in `clippy.toml`)
- Rust edition 2024

## Hardware Pinout (ESP32-C3)

- GPIO4: SPI SCK, GPIO6: SPI MOSI, GPIO5: SPI MISO, GPIO7: SPI CS (CC1101)
- GPIO10: CC1101 GDO0 (packet sync/done interrupt)
- GPIO9: Reset button (active low input)
- CC1101 configured for 868.3 MHz, 2-FSK, 48-byte packets, wMBus C1 mode sync word 0x543D
