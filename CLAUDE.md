# Claude Repository Notes

Use `AGENTS.md` as the canonical repository instruction file for structure, build commands, style, tests, and pull
request guidance.

Current maintenance notes:

- Firmware targets ESP32-C3 by default and ESP-WROOM-32 with `--no-default-features --features=esp-wroom-32`.
- ESP-IDF is pinned to `v5.5.4` in `.cargo/config.toml`; do not move to ESP-IDF 6.x without updating and validating the Rust ESP-IDF crate stack.
- Dependency checks from 2026-06-12 found no direct dependency updates and no lockfile updates. The only newer reported crate was transitive `matchit` (`0.8.4` via `axum 0.8.9`, latest `0.8.6`).
- For dependency work, run `cargo outdated --workspace --root-deps-only`, `cargo outdated --workspace`, and `cargo update --dry-run --verbose` before editing `Cargo.toml` or `Cargo.lock`.
