// lib.rs

#![warn(clippy::large_futures)]

pub use std::{net, pin::Pin, sync::Arc};

pub use anyhow::bail;
pub use askama::Template;
pub use chrono::*;
pub use esp_idf_hal::{
    delay::FreeRtos,
    gpio::{AnyInputPin, Input, InputPin, PinDriver},
    prelude::*,
    spi,
};
pub use log::*;
pub use serde::{Deserialize, Serialize};
pub use tokio::time::{sleep, timeout, Duration};

pub const FW_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, Serialize)]
pub struct MeterReading {
    pub total_m3: f32,
    pub target_m3: f32,
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

pub mod cc1101;
pub use cc1101::Cc1101Radio;

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

mod mqtt;
pub use mqtt::*;

mod apiserver;
pub use apiserver::*;

mod wifi;
pub use wifi::*;

// EOF
