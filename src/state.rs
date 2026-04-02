// state.rs

use crate::*;

pub const AP_MODE_NVS_KEY: &str = "boot_ap";

pub struct MyState {
    pub ap_mode: bool,
    pub ota_slot: String,
    pub config: RwLock<MyConfig>,
    pub uptime: RwLock<usize>,
    pub api_cnt: AtomicU32,
    pub net_up: RwLock<bool>,
    pub if_index: RwLock<u32>,
    pub ip_addr: RwLock<net::Ipv4Addr>,
    pub ping_ip: RwLock<Option<net::Ipv4Addr>>,
    pub my_id: RwLock<String>,
    pub my_mac: RwLock<[u8; 6]>,
    pub my_mac_s: RwLock<String>,
    pub latest_data: RwLock<Option<MeterReading>>,
    pub data_updated: RwLock<bool>,
    pub nvs: RwLock<nvs::EspNvs<nvs::NvsDefault>>,
    pub led: RwLock<PinDriver<'static, Output>>,
    pub reset: RwLock<bool>,
}

impl MyState {
    pub fn new(
        ap_mode: bool,
        config: MyConfig,
        nvs: nvs::EspNvs<nvs::NvsDefault>,
        ota_slot: String,
        led: PinDriver<'static, Output>,
    ) -> Self {
        MyState {
            ap_mode,
            ota_slot,
            config: RwLock::new(config),
            uptime: RwLock::new(0),
            api_cnt: 0.into(),
            net_up: RwLock::new(false),
            if_index: RwLock::new(0),
            ip_addr: RwLock::new(net::Ipv4Addr::new(0, 0, 0, 0)),
            ping_ip: RwLock::new(None),
            my_id: RwLock::new("esp32multical_000000000000".into()),
            my_mac: RwLock::new([0, 0, 0, 0, 0, 0]),
            my_mac_s: RwLock::new("00:00:00:00:00:00".into()),
            latest_data: RwLock::new(None),
            data_updated: RwLock::new(false),
            nvs: RwLock::new(nvs),
            led: RwLock::new(led),
            reset: RwLock::new(false),
        }
    }

    pub async fn set_led(&self, enabled: bool) -> AppResult<()> {
        let mut led = self.led.write().await;
        if enabled != LED_ACTIVE_LOW {
            led.set_high()?;
        } else {
            led.set_low()?;
        }
        Ok(())
    }

    pub async fn led_on(&self) -> AppResult<()> {
        self.set_led(true).await
    }
    pub async fn led_off(&self) -> AppResult<()> {
        self.set_led(false).await
    }

    pub async fn request_ap_mode_on_next_boot(&self) -> AppResult<()> {
        self.nvs.write().await.set_u8(AP_MODE_NVS_KEY, 1)?;
        Ok(())
    }
}
// EOF
