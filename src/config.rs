// config.rs

use crc::{CRC_32_ISCSI, Crc};

use crate::*;

pub const NVS_BUF_SIZE: usize = 256;
pub const HTTP_API_PORT: u16 = 80;
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

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_hex<const N: usize>(hex: &str) -> Option<[u8; N]> {
    let bytes = hex.as_bytes();
    if bytes.len() != N.checked_mul(2)? {
        return None;
    }

    let mut output = [0_u8; N];
    for (destination, pair) in output.iter_mut().zip(bytes.chunks_exact(2)) {
        let &[high, low] = pair else {
            return None;
        };
        *destination = (hex_nibble(high)? << 4) | hex_nibble(low)?;
    }

    Some(output)
}

impl MyConfig {
    /// Parse meter_id hex string (8 hex chars) to 4 bytes in wire order.
    /// The meter ID is entered as printed on the meter (big-endian),
    /// but the wire format is little-endian, so we reverse the bytes.
    pub fn meter_id_bytes(&self) -> Option<[u8; 4]> {
        let mut bytes = parse_hex::<4>(&self.meter_id)?;
        bytes.reverse();
        Some(bytes)
    }

    /// Parse meter_key hex string (32 hex chars) to 16 bytes.
    pub fn meter_key_bytes(&self) -> Option<[u8; 16]> {
        parse_hex::<16>(&self.meter_key)
    }

    pub fn from_nvs(nvs: &mut nvs::EspNvs<nvs::NvsDefault>) -> Option<Self> {
        let mut nvsbuf = [0u8; NVS_BUF_SIZE];
        info!("Reading up to {sz} bytes from nvs...", sz = NVS_BUF_SIZE);
        let b = match nvs.get_blob(CONFIG_NAME, &mut nvsbuf) {
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

        nvs.set_blob(CONFIG_NAME, nvsdata)
            .map_err(|e| AppError::Message(format!("Cannot save to nvs: {e:?}")))?;
        info!("Config saved.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_hex;

    #[test]
    fn parse_hex_accepts_upper_and_lower_case() {
        assert_eq!(parse_hex::<4>("01aBfe90"), Some([0x01, 0xab, 0xfe, 0x90]));
    }

    #[test]
    fn parse_hex_rejects_wrong_length_and_invalid_characters() {
        assert_eq!(parse_hex::<4>("1234"), None);
        assert_eq!(parse_hex::<4>("1234567g"), None);
    }

    #[test]
    fn parse_hex_rejects_non_ascii_without_panicking() {
        assert_eq!(parse_hex::<4>("€aaaaa"), None);
    }
}

// EOF
