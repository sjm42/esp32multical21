# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ESP32 Multical21 — a Rust embedded IoT device for ESP32-C3 (default) or ESP-WROOM-32. It reads wireless M-Bus meter data via a CC1101 868 MHz RF module and exposes a web configuration UI, REST API, MQTT publisher, and optional ESPHome native API. It also supports a fixed AP-mode recovery/configuration path for local setup. Runs on ESP-IDF with Tokio async runtime.

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
cargo clippy --all-targets --all-features

# Target ESP-WROOM-32 instead of the default ESP32-C3
cargo build --features esp-wroom-32 --no-default-features
```

The build target is `riscv32imc-esp-espidf` (configured in `.cargo/config.toml`). The toolchain is nightly with `rust-src` for custom std builds against ESP-IDF v5.4.3. Flash uses dual OTA partition table (`partitions.csv`) with `--erase-parts otadata` to reset OTA tracking on fresh flash.

## Architecture

The binary entry point is `src/bin/esp32multical21.rs`. It initializes hardware (SPI for CC1101 RF module, GPIO for reset button and onboard LED), loads config and AP-mode boot flags from NVS, sets up WiFi, then runs seven concurrent tasks via `tokio::select!`:

| Task | Module | Purpose |
|------|--------|---------|
| `poll_reset()` | bin | uptime tracking, short-press AP-mode request, long-press factory reset |
| `read_meter()` | `measure.rs` | CC1101 radio reception, wMBus decoding, meter data parsing |
| `run_mqtt()` | `mqtt_sender.rs` | MQTT client, publishes to `{topic}/uptime` and `{topic}/meter` |
| `run_api_server()` | `apiserver.rs` | Axum HTTP server (port 80 default) |
| `run_esphome_api()` | `esphome_api.rs` | ESPHome native API server (port 6053) |
| `wifi_loop.run()` | `wifi.rs` | WiFi station/AP-mode manager |
| `pinger()` | bin | gateway ping watchdog (station mode) |

**Shared state** (`state.rs`): `MyState` struct with Tokio `RwLock` fields (config, uptime, WiFi status, IP, LED driver, etc.) wrapped in `Arc` and passed to all tasks.

**Configuration** (`config.rs`): `MyConfig` struct serialized with Postcard + CRC-32 into ESP-IDF NVS. Covers WiFi, static IP/DHCP, MQTT, ESPHome, meter ID and AES key. Defaults can be overridden via env vars (`WIFI_SSID`, `WIFI_PASS`).

**Web UI**: Askama template (`templates/index.html.ask`) renders a config form. Static assets are gzip-compressed at build time and embedded into the firmware.

**API routes** (Axum, in `apiserver.rs`):
- `GET /` — config page, `GET /uptime` — JSON uptime, `GET /conf` — JSON config
- `POST /conf` — save config & reboot, `GET /reset_conf` — factory reset
- `GET /meter` — JSON current meter reading
- `POST /fw` — OTA firmware update (accepts form field `url` pointing to firmware binary)

In AP mode, the local HTTP config UI remains available at `http://10.42.42.1/`, but meter reading, MQTT, and ESPHome API are disabled.

## AP Mode And Button Behavior

- Short button press requests one-shot AP mode and reboots.
- AP mode comes up as open SSID `esp32multical21` on `10.42.42.1/24`.
- Long button press of about 5 seconds factory-resets config and reboots.
- While the button is held, the LED blinks; once factory reset starts, the LED stays on until reboot.
- In AP mode, the LED stays on continuously.

## Key Conventions

- Device ID derived from MAC address: `esp32multical21_XXXXXXXXXXXX`
- MQTT client ID follows same pattern as device ID
- Release profile uses `opt-level = "z"` (size) with fat LTO
- Clippy `future-size-threshold = 128`
- Rust edition 2024

## Hardware Pinout

- `esp32-c3`: GPIO9 button, GPIO4/6/5/7 SPI, GPIO10 GDO0, GPIO8 LED (active low)
- `esp-wroom-32`: GPIO0 button, GPIO18/23/19/5 SPI, GPIO4 GDO0, GPIO2 LED (active high)
- CC1101 is configured for 868.949708 MHz, 2-FSK, 48-byte packets, wMBus C1 mode sync word `0x543D`
