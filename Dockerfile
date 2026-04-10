FROM espressif/idf-rust:esp32c3_latest

# esp-idf-sys downloads ESP-IDF + toolchain here; keep it in a volume for caching
ENV IDF_TOOLS_PATH=/cache/espressif
ENV CARGO_HOME=/cache/cargo

# Install rust-src (required by .cargo/config.toml build-std)
RUN rustup component add rust-src

WORKDIR /project

ARG WIFI_SSID=internet
ARG WIFI_PASS=
ENV WIFI_SSID=${WIFI_SSID}
ENV WIFI_PASS=${WIFI_PASS}

CMD ["cargo", "build", "-r"]
