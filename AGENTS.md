# Repository Guidelines

## Project Structure & Module Organization
This repository contains Rust firmware for ESP32 + CC1101.

- `src/bin/esp32multical21.rs`: firmware entry point and task orchestration.
- `src/*.rs`: core modules (`apiserver`, `wifi`, `mqtt`, `wmbus`, `multical21`, etc.).
- `templates/`: Askama HTML templates used by the web UI.
- `.cargo/config.toml`: chip target, runner, and ESP-IDF build settings.
- `partitions.csv`, `sdkconfig.defaults`: flash layout and ESP-IDF defaults.
- Helper scripts: `flash` (flash+monitor) and `makeimage` (build + `firmware.bin` export).

## Build, Test, and Development Commands
Run from repository root:

- `source env.sh`: set build-time defaults (`MCU`, `WIFI_SSID`, `WIFI_PASS`, `API_PORT`).
- `cargo build`: debug build for configured target.
- `cargo build -r`: release build (size-optimized, LTO enabled).
- `cargo run -r -- --baud 921600`: build, flash, and open monitor via `espflash`.
- `./flash`: shortcut for the same release flash flow.
- `./makeimage`: produce `firmware.bin` for OTA/manual distribution.
- `cargo clippy --all-targets --all-features`: lint before submitting changes.

## Coding Style & Naming Conventions
- Rust edition is `2024`; toolchain is nightly (`rust-toolchain.toml`).
- Format with `cargo fmt` (configured in `rustfmt.toml`: max width 120, grouped imports).
- Keep modules/files `snake_case`; types/traits `CamelCase`; constants `SCREAMING_SNAKE_CASE`.
- Prefer small async functions and avoid large futures (Clippy threshold configured to 128 bytes).

## Testing Guidelines
There is currently no dedicated `tests/` suite. For each change:

- Run `cargo check` and `cargo clippy --all-targets --all-features`.
- Validate on hardware when behavior touches radio, Wi-Fi, OTA, or MQTT.
- If adding pure parsing/business logic, add inline unit tests (`#[cfg(test)]`) near the module.

## Commit & Pull Request Guidelines
Recent history uses short, imperative messages (for example, `cargo update`, `Cleanup & refactoring`).

- Keep commit subjects concise and imperative; one logical change per commit.
- PRs should include: purpose, key technical changes, validation steps/commands, and hardware test notes.
- Link related issues and include API/UI screenshots only when endpoints or templates changed.
