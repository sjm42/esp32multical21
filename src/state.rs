// state.rs

use crate::*;

pub struct MyState {
    pub ota_slot: String,
    pub config: RwLock<MyConfig>,
    pub uptime: RwLock<usize>,
    pub api_cnt: AtomicU32,
    pub wifi_up: RwLock<bool>,
    pub if_index: RwLock<u32>,
    pub ip_addr: RwLock<net::Ipv4Addr>,
    pub ping_ip: RwLock<Option<net::Ipv4Addr>>,
    pub my_id: RwLock<String>,
    pub my_mac: RwLock<[u8; 6]>,
    pub my_mac_s: RwLock<String>,
    pub latest_data: RwLock<Option<MeterReading>>,
    pub data_updated: RwLock<bool>,
    pub nvs: RwLock<nvs::EspNvs<nvs::NvsDefault>>,
    pub reset: RwLock<bool>,
}

impl MyState {
    pub fn new(config: MyConfig, nvs: nvs::EspNvs<nvs::NvsDefault>, ota_slot: String) -> Self {
        MyState {
            ota_slot,
            config: RwLock::new(config),
            uptime: RwLock::new(0),
            api_cnt: 0.into(),
            wifi_up: RwLock::new(false),
            if_index: RwLock::new(0),
            ip_addr: RwLock::new(net::Ipv4Addr::new(0, 0, 0, 0)),
            ping_ip: RwLock::new(None),
            my_id: RwLock::new("esp32multical_000000000000".into()),
            my_mac: RwLock::new([0, 0, 0, 0, 0, 0]),
            my_mac_s: RwLock::new("00:00:00:00:00:00".into()),
            latest_data: RwLock::new(None),
            data_updated: RwLock::new(false),
            nvs: RwLock::new(nvs),
            reset: RwLock::new(false),
        }
    }
}
// EOF
