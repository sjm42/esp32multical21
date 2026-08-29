ARG ESP_BOARD=esp32c3
FROM espressif/idf-rust:all_latest

ARG ESP_BOARD

# Keep downloaded Cargo crates in a volume across builds.
ENV CARGO_HOME=/cache/cargo

# The all-target image supplies the repository-selected `esp` toolchain for
# both the RISC-V C3 and Xtensa WROOM32 builds.

WORKDIR /project

ENV WIFI_SSID=internet
ENV WIFI_PASS=

CMD ["cargo", "build", "-r"]
