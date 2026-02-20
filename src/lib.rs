// lib.rs

#![warn(clippy::large_futures)]

pub use std::{any::Any, net, pin::Pin, sync::Arc};

pub use anyhow::bail;
pub use askama::Template;
pub use chrono::*;
pub use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{AnyInputPin, Input, InputPin, PinDriver},
    prelude::*,
    spi,
};
pub use esp_idf_svc::{
    eventloop::{EspEventLoop, System},
    http::client::EspHttpConnection,
    io, ipv4, mqtt,
    netif::{self, EspNetif},
    nvs,
    ota::EspOta,
    sntp,
    timer::{EspTimerService, Task},
    wifi::{AsyncWifi, EspWifi, WifiDriver},
};
pub use esp_idf_sys::EspError;
pub use log::*;
pub use serde::{Deserialize, Serialize};
pub use tokio::{
    sync::RwLock,
    time::{sleep, timeout, Duration},
};

pub const FW_VERSION: &str = env!("CARGO_PKG_VERSION");

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("ESP-IDF error: {0}")]
    Esp(#[from] esp_idf_sys::EspError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Address parse error: {0}")]
    AddrParse(#[from] std::net::AddrParseError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Radio error: {0}")]
    Radio(#[from] crate::radio::Cc1101RadioError),
    #[error("{0}")]
    Message(String),
}

#[derive(Clone, Debug, Serialize)]
pub struct MeterReading {
    pub total_l: u32,
    pub month_start_l: u32,
    pub total_m3: f32,
    pub month_start_m3: f32,
    pub flow_temp: u8,
    pub ambient_temp: u8,
    pub info_codes: u8,
    pub timestamp: i64,
    pub timestamp_s: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Uptime {
    pub uptime: usize,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFirmware {
    pub url: String,
}

pub mod radio;
pub use radio::Cc1101Radio;

mod wmbus;
pub use wmbus::*;

mod multical21;
pub use multical21::*;

mod config;
pub use config::*;

mod state;
pub use state::*;

mod measure;
pub use measure::*;

mod mqtt_sender;
pub use mqtt_sender::*;

mod apiserver;
pub use apiserver::*;

mod esphome_api;
pub use esphome_api::*;

mod wifi;
pub use wifi::*;

// EOF
