ARG ESP_BOARD=esp32c3
FROM espressif/idf-rust:${ESP_BOARD}_latest

ARG ESP_BOARD

# Keep downloaded Cargo crates in a volume across builds.
ENV CARGO_HOME=/cache/cargo

# The C3 target uses upstream nightly; the Xtensa image already supplies its
# custom `esp` toolchain and does not support installing standard components.
RUN if [ "$ESP_BOARD" = "esp32c3" ]; then rustup component add rust-src clippy; fi

WORKDIR /project

ENV WIFI_SSID=internet
ENV WIFI_PASS=

CMD ["cargo", "build", "-r"]
