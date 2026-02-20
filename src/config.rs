// config.rs

use crc::{Crc, CRC_32_ISCSI};

use crate::*;

pub const NVS_BUF_SIZE: usize = 256;

pub const DEFAULT_API_PORT: u16 = 80;

const CONFIG_NAME: &str = "cfg";

#[derive(Clone, Debug, Serialize, Deserialize, Template)]
#[template(path = "index.html.ask", escape = "html")]
pub struct MyConfig {
    pub wifi_ssid: String,
    pub wifi_pass: String,
    pub wifi_wpa2ent: bool,
    pub wifi_username: String,

    pub v4dhcp: bool,
    pub v4addr: net::Ipv4Addr,
    pub v4mask: u8,
    pub v4gw: net::Ipv4Addr,
    pub dns1: net::Ipv4Addr,
    pub dns2: net::Ipv4Addr,

    pub esphome_enable: bool,
    pub mqtt_enable: bool,
    pub mqtt_url: String,
    pub mqtt_topic: String,

    pub meter_id: String,
    pub meter_key: String,
}

impl Default for MyConfig {
    fn default() -> Self {
        Self {
            wifi_ssid: option_env!("WIFI_SSID").unwrap_or("internet").into(),
            wifi_pass: option_env!("WIFI_PASS").unwrap_or("").into(),
            wifi_wpa2ent: false,
            wifi_username: String::new(),

            esphome_enable: false,
            v4dhcp: true,
            v4addr: net::Ipv4Addr::new(0, 0, 0, 0),
            v4mask: 0,
            v4gw: net::Ipv4Addr::new(0, 0, 0, 0),
            dns1: net::Ipv4Addr::new(0, 0, 0, 0),
            dns2: net::Ipv4Addr::new(0, 0, 0, 0),

            mqtt_enable: false,
            mqtt_url: "mqtt://mqtt.local:1883".into(),
            mqtt_topic: "watermeter".into(),

            meter_id: String::new(),
            meter_key: String::new(),
        }
    }
}

fn parse_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

impl MyConfig {
    /// Parse meter_id hex string (8 hex chars) to 4 bytes in wire order.
    /// The meter ID is entered as printed on the meter (big-endian),
    /// but the wire format is little-endian, so we reverse the bytes.
    pub fn meter_id_bytes(&self) -> Option<[u8; 4]> {
        if self.meter_id.len() != 8 {
            return None;
        }
        let bytes = parse_hex(&self.meter_id)?;
        Some([bytes[3], bytes[2], bytes[1], bytes[0]])
    }

    /// Parse meter_key hex string (32 hex chars) to 16 bytes.
    pub fn meter_key_bytes(&self) -> Option<[u8; 16]> {
        if self.meter_key.len() != 32 {
            return None;
        }
        let bytes = parse_hex(&self.meter_key)?;
        let mut arr = [0u8; 16];
        arr.copy_from_slice(&bytes);
        Some(arr)
    }

    pub fn from_nvs(nvs: &mut nvs::EspNvs<nvs::NvsDefault>) -> Option<Self> {
        let mut nvsbuf = [0u8; NVS_BUF_SIZE];
        info!("Reading up to {sz} bytes from nvs...", sz = NVS_BUF_SIZE);
        let b = match nvs.get_raw(CONFIG_NAME, &mut nvsbuf) {
            Err(e) => {
                error!("Nvs read error {e:?}");
                return None;
            }
            Ok(Some(b)) => b,
            _ => {
                error!("Nvs key not found");
                return None;
            }
        };
        info!("Got {sz} bytes from nvs. Parsing config...", sz = b.len());

        let crc = Crc::<u32>::new(&CRC_32_ISCSI);
        let digest = crc.digest();
        match postcard::from_bytes_crc32::<MyConfig>(b, digest) {
            Ok(c) => {
                info!("Successfully parsed config from nvs.");
                Some(c)
            }
            Err(e) => {
                error!("Cannot parse config from nvs: {e:?}");
                None
            }
        }
    }

    pub fn to_nvs(&self, nvs: &mut nvs::EspNvs<nvs::NvsDefault>) -> AppResult<()> {
        let mut nvsbuf = [0u8; NVS_BUF_SIZE];
        let crc = Crc::<u32>::new(&CRC_32_ISCSI);
        let digest = crc.digest();
        let nvsdata = postcard::to_slice_crc32(self, &mut nvsbuf, digest)
            .map_err(|e| AppError::Message(format!("Cannot encode config to buffer {e:?}")))?;
        info!("Encoded config to {sz} bytes. Saving to nvs...", sz = nvsdata.len());

        nvs.set_raw(CONFIG_NAME, nvsdata)
            .map_err(|e| AppError::Message(format!("Cannot save to nvs: {e:?}")))?;
        info!("Config saved.");
        Ok(())
    }
}

// EOF
