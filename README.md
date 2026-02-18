# ESP32 Multical21

A Rust embedded firmware for ESP32-C3 (or ESP32-S2) that receives encrypted wireless M-Bus (wMBus)
telegrams from Kamstrup Multical 21 water meters via a CC1101 868 MHz RF module.
Decoded meter readings are exposed through a web UI, REST API, and MQTT.

Runs on Tokio async runtime on top of FreeRTOS.

## Hardware

### Components

- **ESP32-C3** (RISC-V) or ESP32-S2 (Xtensa) microcontroller with minimum 4MB flash
- **CC1101** sub-GHz RF transceiver module (868.3 MHz, 2-FSK)


### Pinout (ESP32-C3)

| Pin    | Function            |
|--------|---------------------|
| GPIO4  | SPI SCK             |
| GPIO5  | SPI MISO            |
| GPIO6  | SPI MOSI            |
| GPIO7  | SPI CS (CC1101)     |
| GPIO10 | CC1101 GDO0 (IRQ)  |
| GPIO9  | Reset button (active low) |

The CC1101 is configured for wMBus C1 mode: 868.3 MHz, 2-FSK modulation, sync word `0x543D`,
48-byte packets. GDO0 asserts on sync word detection and deasserts when the packet is complete.

## Building & Flashing

### Prerequisites

- Nightly Rust toolchain with `rust-src` component (see `rust-toolchain.toml`)
- ESP-IDF tooling: `espflash`, `ldproxy`, `embuild`


### Commands

```bash
source env.sh                          # Set WIFI_SSID, WIFI_PASS, MCU, API_PORT

cargo build                            # Debug build
cargo build -r                         # Release build (opt-level=z, fat LTO)
cargo clippy                           # Lint

cargo run -r -- --baud 921600          # Build release + flash + monitor
./flash                                # Shortcut for the above
```

The flash runner (configured in `.cargo/config.toml`) uses `espflash` with the dual-OTA partition table and erases OTA metadata on each fresh flash:

```
espflash flash --monitor --partition-table ./partitions.csv --erase-parts otadata
```

### Targeting ESP32-S2

```bash
cargo build --no-default-features --features esp32s 
```

The default feature is `esp32c3` (RISC-V target `riscv32imc-esp-espidf`).

## Configuration

Device configuration is persisted in NVS (Non-Volatile Storage) as a Postcard-serialized blob with CRC-32 integrity checking.

| Parameter     | Description                              | Default        |
|---------------|------------------------------------------|----------------|
| `port`        | HTTP server port                         | 80             |
| `wifi_ssid`   | WiFi SSID                                | from `env.sh`  |
| `wifi_pass`   | WiFi password                            | from `env.sh`  |
| `v4dhcp`      | Use DHCP                                 | true           |
| `v4addr`      | Static IPv4 address                      | 0.0.0.0        |
| `v4mask`      | Subnet mask bits (0-30)                  | 0              |
| `v4gw`        | Gateway                                  | 0.0.0.0        |
| `dns1`/`dns2` | DNS servers                              | 0.0.0.0        |
| `mqtt_enable` | Enable MQTT publishing                   | false          |
| `mqtt_url`    | MQTT broker URL                          | (empty)        |
| `mqtt_topic`  | MQTT topic prefix                        | (empty)        |
| `meter_id`    | Target meter serial (8 hex chars)        | (empty)        |
| `meter_key`   | AES-128 decryption key (32 hex chars)    | (empty)        |

Configuration can be changed through the web UI at `http://<device-ip>/` or via `POST /conf` with a JSON body.
Changes take effect after an automatic reboot.

Environment variables `WIFI_SSID`, `WIFI_PASS`, and `API_PORT` provide build-time defaults.

## Home Assistant integration via MQTT

Setup your MQTT broker first.

To your main config `configuration.yaml` you probably want to add:
```
mqtt: !include_dir_list mqtt
```

Add this to your `mqtt` subdirectory, as `watermeter.yaml` or whatever filename you like:

```
sensor:
  - name: "Water Meter Usage"
    unique_id: "water_total"
    state_topic: "watermeter/meter"
    unit_of_measurement: "m³"
    value_template: "{{ value_json.total_m3 }}"
    device_class: water
    state_class: total_increasing
  - name: "Water Meter Room Temperature"
    unique_id: "water_temp_room"
    state_topic: "watermeter/meter"
    value_template: "{{ value_json.ambient_temp }}"
    unit_of_measurement: "°C"
  - name: "Water Meter Water Temperature"
    unique_id: "water_temp_water"
    state_topic: "watermeter/meter"
    value_template: "{{ value_json.flow_temp }}"
    unit_of_measurement: "°C"
  - name: "Water Meter uptime"
    unique_id: "water_uptime"
    state_topic: "watermeter/uptime"
    value_template: "{{ value_json.uptime }}"
    unit_of_measurement: "s"
```

## HTTP API

Served by Axum on port 80 (configurable).

| Method  | Path           | Description                          |
|---------|----------------|--------------------------------------|
| GET     | `/`            | Web configuration UI (Askama template) |
| GET     | `/uptime`      | `{"uptime": <seconds>}`             |
| GET     | `/conf`        | Current config as JSON               |
| POST    | `/conf`        | Save config and reboot               |
| GET     | `/reset_conf`  | Factory reset and reboot             |
| GET     | `/meter`       | Current meter reading as JSON        |
| POST    | `/fw`          | OTA firmware update (form field `url`) |

## OTA Firmware Update

The flash is partitioned into two 1984 KB OTA slots (`ota_0`, `ota_1`). To update:

1. Host the new firmware binary on an HTTP server
2. POST to `/fw` with form field `url` pointing to the binary
3. The device downloads the firmware, writes it to the inactive OTA slot, and reboots
4. On boot, the new firmware calls `mark_running_slot_valid()`
   — if it crashes before doing so, the bootloader automatically rolls back to the previous slot

### Partition Table

```
nvs,      data, nvs,   0x9000,  0x4000   (16 KB)
otadata,  data, ota,   0xd000,  0x2000   (8 KB)
phy_init, data, phy,   0xf000,  0x1000   (4 KB)
ota_0,    app,  ota_0, ,        1984K
ota_1,    app,  ota_1, ,        1984K
```

## Watchdogs & Recovery

- **Reset button**: Hold GPIO9 low for 5 seconds to factory-reset configuration and reboot
- **WiFi watchdog**: If initial WiFi connection fails within 30 seconds, the device reboots
- **Ping watchdog**: Every 5 minutes, pings the gateway 3 times. If all fail, reboots
- **Radio watchdog**: If no packet is received for 5 minutes, the CC1101 is reinitialized
- **OTA rollback**: If new firmware fails to mark itself valid, the bootloader reverts to the previous slot

## Build Configuration

- **Rust edition**: 2024 (nightly)
- **Release profile**: `opt-level = "z"` (size-optimized), fat LTO, single codegen unit
- **ESP-IDF**: v5.4.3, main task stack 20 KB, FreeRTOS tick rate 1 kHz
- **Clippy**: `future-size-threshold = 128` to catch oversized futures

### Meter Reading Response

```json
{
  "total_volume_l": 123456,
  "target_volume_l": 120000,
  "flow_temp": 22,
  "ambient_temp": 20,
  "info_codes": 0,
  "timestamp": "2025-01-15T12:30:00Z"
}
```

The web UI polls `/uptime` and `/meter` every 10 seconds and renders a live dashboard.

## MQTT

When enabled, the device connects to the configured MQTT broker and publishes every 60 seconds when new data is available:

- **`{topic}/uptime`** — `{"uptime": <seconds>}`
- **`{topic}/meter`** — `{"total_m3": <f64>, "target_m3": <f64>, "flow_temp": <u8>, "ambient_temp": <u8>, "info_codes": <u8>, "uptime": <usize>}`

Volumes are published in cubic meters (converted from liters).
The MQTT client ID is derived from the device MAC address: `esp32multical21-XX:XX:XX:XX:XX:XX`.

## wMBus Protocol

The CC1101 radio listens for wireless M-Bus C1 mode telegrams at 868.3 MHz. When a packet arrives:

1. **Sync detection** — CC1101 matches the C1 preamble `0x543D`
2. **Meter ID filtering** — Only packets matching the configured meter serial are processed
3. **AES-128-CTR decryption** — The 16-byte IV is constructed from the frame header fields (manufacturer, address, communication control, session number)
4. **CRC-16 validation** — EN 13757 polynomial `0x3D65` verifies payload integrity
5. **Payload parsing** — Multical 21 compact (CI `0x79`) or long (CI `0x78`) frame format extracts volume, temperature, and status data

### Frame Structure

```
Over the air:
[Preamble 0x543D] [L] [C] [M-field 2B] [A-field 6B] [CI] [CC] [ACC] [SN 4B] [Encrypted Payload] [CRC]

After decryption:
[CRC-16 2B] [CI] [Info] ... [Total Volume 4B] ... [Target Volume 4B] ... [Flow Temp] [Ambient Temp]
```

The meter ID is encoded in little-endian BCD on the wire
— a meter printing serial `12345678` transmits bytes `[0x78, 0x56, 0x34, 0x12]`.

## Architecture

The binary entry point (`src/bin/esp32multical21.rs`) initializes hardware, loads config from NVS,
connects to WiFi, then runs six concurrent tasks under `tokio::select!`:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Tokio Runtime (single-threaded)               │
│                                                                 │
│  poll_reset()     Uptime counter, factory-reset button (2s)     │
│  poll_sensors()   CC1101 RX → wMBus decrypt → meter parse       │
│  run_mqtt()       Publish meter data to MQTT broker (5s)        │
│  run_api_server() Axum HTTP server (port 80)                    │
│  wifi_loop.run()  WiFi connect/reconnect manager                │
│  pinger()         Ping gateway every 5 min, reboot on failure   │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              │  Shared State (Arc<MyState>)   │
              │  RwLock fields for config,     │
              │  uptime, wifi, meter, etc.     │
              └───────────────────────────────┘
```

All tasks share a single `Arc<Pin<Box<MyState>>>` instance with `RwLock`-protected fields.

### Source Modules

| File               | Purpose                                            |
|--------------------|----------------------------------------------------|
| `bin/esp32multical21.rs` | Entry point, hardware init, task orchestration |
| `lib.rs`           | Re-exports, common types, `FW_VERSION` constant    |
| `state.rs`         | `MyState` struct — shared concurrent state         |
| `config.rs`        | `MyConfig` struct — NVS serialization/deserialization |
| `cc1101.rs`        | CC1101 SPI driver — register config, packet RX     |
| `wmbus.rs`         | wMBus C1 frame parsing, AES-128-CTR decryption     |
| `multical21.rs`    | Kamstrup Multical 21 payload parser                |
| `measure.rs`       | Sensor polling loop — ties radio to state          |
| `mqtt.rs`          | MQTT client lifecycle and publishing               |
| `apiserver.rs`     | Axum HTTP routes, web UI, OTA updates              |
| `wifi.rs`          | WiFi connection/reconnection state machine         |

### Startup Sequence

1. Initialize ESP-IDF (logging, eventfd VFS, system event loop)
2. Load `MyConfig` from NVS (or save defaults if first boot)
3. Initialize OTA subsystem, mark running slot valid (prevents rollback)
4. Configure SPI bus and CC1101 radio, set up GPIO for reset button
5. Create WiFi driver and shared `MyState`
6. Launch Tokio runtime with six concurrent tasks
7. WiFi connects (30s timeout, reboots on failure), then all services start


## License

MIT

Author: Sami J. Mäkinen <sjm@iki.fi>

This is originally based on C++ code here: https://github.com/pthalin/esp32-multical21

Thanks to __pthalin__ for the effort.
