// wifi.rs

use embedded_svc::wifi::{AccessPointConfiguration, AuthMethod, ClientConfiguration, Configuration};
use esp_idf_svc::wifi::WifiEvent;

use crate::*;

pub struct WifiLoop<'a> {
    pub state: Arc<std::pin::Pin<Box<MyState>>>,
    pub wifi: Option<AsyncWifi<EspWifi<'a>>>,
}

impl<'a> WifiLoop<'a> {
    pub async fn run(
        mut self,
        wifidriver: WifiDriver<'_>,
        sysloop: EspEventLoop<System>,
        timer: EspTimerService<Task>,
    ) -> AppResult<()> {
        info!("Initializing Wi-Fi...");

        let _disconnect_sub = sysloop.subscribe::<WifiEvent, _>(|event| {
            if let WifiEvent::StaDisconnected(d) = event {
                warn!(
                    "WiFi disconnected: reason={} ({}) rssi={}",
                    d.reason(),
                    wifi_disconnect_reason(d.reason()),
                    d.rssi()
                );
            }
        })?;

        let net_if = if self.state.ap_mode {
            EspNetif::new_with_conf(&netif::NetifConfiguration::wifi_default_client())?
        } else {
            let config = self.state.config.read().await.clone();
            let ipv4_config = if config.v4dhcp {
                ipv4::ClientConfiguration::DHCP(ipv4::DHCPClientSettings {
                    hostname: Some("esp32multical21".try_into().unwrap()),
                    ..Default::default()
                })
            } else {
                ipv4::ClientConfiguration::Fixed(ipv4::ClientSettings {
                    ip: config.v4addr,
                    subnet: ipv4::Subnet {
                        gateway: config.v4gw,
                        mask: ipv4::Mask(config.v4mask),
                    },
                    dns: Some(config.dns1),
                    secondary_dns: Some(config.dns2),
                })
            };

            EspNetif::new_with_conf(&netif::NetifConfiguration {
                ip_configuration: Some(ipv4::Configuration::Client(ipv4_config)),
                ..netif::NetifConfiguration::wifi_default_client()
            })?
        };

        let mac = net_if.get_mac()?;
        *self.state.my_id.write().await = format!(
            "esp32multical21_{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5],
        );
        *self.state.my_mac.write().await = mac;
        *self.state.my_mac_s.write().await = format!(
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5],
        );

        let ap_netif = EspNetif::new_with_conf(&netif::NetifConfiguration {
            ip_configuration: Some(ipv4::Configuration::Router(ipv4::RouterConfiguration {
                subnet: ipv4::Subnet {
                    gateway: AP_MODE_IP_ADDR,
                    mask: ipv4::Mask(AP_MODE_IP_MASK),
                },
                dhcp_enabled: true,
                dns: Some(AP_MODE_IP_ADDR),
                secondary_dns: None,
            })),
            ..netif::NetifConfiguration::wifi_default_router()
        })?;

        let espwifi = EspWifi::wrap_all(wifidriver, net_if, ap_netif)?;
        self.wifi = Some(AsyncWifi::wrap(espwifi, sysloop, timer.clone())?);

        if self.state.ap_mode {
            return Box::pin(self.run_ap_mode()).await;
        }

        Box::pin(self.configure()).await?;

        if let Err(e) = Box::pin(self.initial_connect()).await {
            error!("WiFi connection failed: {e:?}");
            error!("Resetting...");
            sleep(Duration::from_secs(5)).await;
            esp_idf_hal::reset::restart();
        }
        info!("WiFi is connected.");
        sleep(Duration::from_secs(2)).await;

        let netif = self.wifi.as_ref().unwrap().wifi().sta_netif();
        let ip_info = netif.get_ip_info()?;
        *self.state.if_index.write().await = netif.get_index();
        *self.state.ip_addr.write().await = ip_info.ip;
        *self.state.ping_ip.write().await = Some(ip_info.subnet.gateway);

        // wait for NTP synchronization to complete
        let ntp = sntp::EspSntp::new_default()?;
        sleep(Duration::from_secs(5)).await;
        let mut cnt = 0;
        loop {
            if Utc::now().year() > 2020 && ntp.get_sync_status() == sntp::SyncStatus::Completed {
                break;
            }

            if cnt > 120 {
                esp_idf_hal::reset::restart();
            }
            cnt += 1;
            sleep(Duration::from_millis(1000)).await;
        }
        info!("NTP ok.");

        *self.state.net_up.write().await = true;
        Box::pin(self.stay_connected()).await
    }

    async fn run_ap_mode(&mut self) -> AppResult<()> {
        Box::pin(self.configure_ap()).await?;

        let netif = self.wifi.as_ref().unwrap().wifi().ap_netif();
        let ip_info = netif.get_ip_info()?;
        *self.state.if_index.write().await = netif.get_index();
        *self.state.ip_addr.write().await = ip_info.ip;
        *self.state.ping_ip.write().await = None;
        *self.state.net_up.write().await = true;
        self.state.led_on().await?;

        info!("WiFi AP mode is up at {} for manual configuration.", ip_info.ip);

        loop {
            sleep(Duration::from_secs(3600)).await;
        }
    }

    async fn configure_ap(&mut self) -> AppResult<()> {
        info!("WiFi starting access point mode...");
        let wifi = self.wifi.as_mut().unwrap();
        let ap_cfg = AccessPointConfiguration {
            ssid: AP_MODE_SSID
                .try_into()
                .map_err(|e| AppError::Message(format!("Invalid AP SSID: {e:?}")))?,
            auth_method: AuthMethod::None,
            max_connections: 4,
            ..Default::default()
        };

        wifi.set_configuration(&Configuration::AccessPoint(ap_cfg))?;

        info!("WiFi driver starting...");
        Box::pin(wifi.start()).await?;

        info!("WiFi AP ready.");
        Ok(())
    }

    pub async fn configure(&mut self) -> AppResult<()> {
        info!("WiFi setting credentials...");
        let wifi = self.wifi.as_mut().unwrap();
        let config = self.state.config.read().await.clone();

        let mut client_cfg = ClientConfiguration {
            ssid: config
                .wifi_ssid
                .as_str()
                .try_into()
                .map_err(|e| AppError::Message(format!("Invalid WiFi SSID: {e:?}")))?,
            ..Default::default()
        };

        if config.wifi_pass.is_empty() {
            client_cfg.auth_method = AuthMethod::None;
        } else {
            client_cfg.auth_method = AuthMethod::WPA2Personal;
            client_cfg.password = config
                .wifi_pass
                .as_str()
                .try_into()
                .map_err(|e| AppError::Message(format!("Invalid WiFi password: {e:?}")))?;
        }

        if config.wifi_wpa2ent {
            client_cfg.auth_method = AuthMethod::WPA2Enterprise;

            let username = config.wifi_username.as_bytes();
            let password = config.wifi_pass.as_bytes();
            unsafe {
                esp_idf_sys::esp_eap_client_clear_ca_cert();
                esp_idf_sys::esp_eap_client_clear_certificate_and_key();
                esp_idf_sys::esp_eap_client_clear_identity();
                esp_idf_sys::esp_eap_client_clear_username();
                esp_idf_sys::esp_eap_client_clear_password();
                esp_idf_sys::esp_eap_client_clear_new_password();

                let ret1 = esp_idf_sys::esp_eap_client_set_identity(username.as_ptr(), username.len() as i32);
                let ret2 = esp_idf_sys::esp_eap_client_set_username(username.as_ptr(), username.len() as i32);
                let ret3 = esp_idf_sys::esp_eap_client_set_password(password.as_ptr(), password.len() as i32);
                let ret4 = esp_idf_sys::esp_wifi_sta_enterprise_enable();

                info!("WiFi WPA2 Enterprise: {ret1}:{ret2}:{ret3}:{ret4}");
            }
        }

        wifi.set_configuration(&Configuration::Client(client_cfg))?;

        // esp-idf-svc hardcodes pmf_cfg.capable=false, but WPA2/WPA3 mixed-mode APs
        // require the client to advertise PMF capability or the 4-way handshake times out.
        unsafe {
            let mut cfg: esp_idf_sys::wifi_config_t = core::mem::zeroed();
            esp_idf_sys::esp!(esp_idf_sys::esp_wifi_get_config(
                esp_idf_sys::wifi_interface_t_WIFI_IF_STA,
                &mut cfg,
            ))?;
            cfg.sta.pmf_cfg.capable = true;
            esp_idf_sys::esp!(esp_idf_sys::esp_wifi_set_config(
                esp_idf_sys::wifi_interface_t_WIFI_IF_STA,
                &mut cfg,
            ))?;
        }

        info!("WiFi driver starting...");
        Box::pin(wifi.start()).await?;

        info!("WiFi disabling modem power save...");
        esp_idf_sys::esp!(unsafe { esp_idf_sys::esp_wifi_set_ps(esp_idf_sys::wifi_ps_type_t_WIFI_PS_NONE) })?;

        Ok(())
    }

    pub async fn initial_connect(&mut self) -> AppResult<()> {
        self.do_connect_loop(true).await
    }

    pub async fn stay_connected(mut self) -> AppResult<()> {
        self.do_connect_loop(false).await
    }

    async fn do_connect_loop(&mut self, initial: bool) -> AppResult<()> {
        let wifi = self.wifi.as_mut().unwrap();
        loop {
            // Wait for disconnect before trying to connect again.  This loop ensures
            // we stay connected and is commonly missing from trivial examples as it's
            // way too difficult to showcase the core logic of an example and have
            // a proper Wi-Fi event loop without a robust async runtime.  Fortunately, we can do it
            // now!
            Box::pin(wifi.wifi_wait(|w| w.is_up(), None)).await.ok();

            info!("WiFi connecting...");
            Box::pin(wifi.connect()).await.ok();

            // Apply the 30s timeout here — this is the call that actually waits for the
            // connection. The previous wifi_wait returns immediately at boot since WiFi
            // isn't up yet, so the timeout there was effectively useless.
            let timeout = if initial { Some(Duration::from_secs(30)) } else { None };
            info!("WiFi waiting for association...");
            match Box::pin(wifi.ip_wait_while(|w| w.is_up().map(|s| !s), timeout)).await {
                Ok(_) => {}
                Err(e) => {
                    error!("WiFi error: {e:?}");

                    // only exit here if this is initial connection
                    // otherwise, keep trying
                    if initial {
                        return Err(e.into());
                    }
                }
            }

            info!("WiFi connected.");
            if initial {
                return Ok(());
            }
            sleep(Duration::from_secs(5)).await;
        }
    }
}
fn wifi_disconnect_reason(r: u16) -> &'static str {
    match r {
        1 => "UNSPECIFIED",
        2 => "AUTH_EXPIRE",
        3 => "AUTH_LEAVE",
        4 => "DISASSOC_INACTIVE",
        5 => "DISASSOC_AP_BUSY",
        6 => "6WAY_HANDSHAKE_TIMEOUT",
        7 => "DISASSOC_STA_HAS_LEFT",
        8 => "STA_DISASSOC",
        15 => "4WAY_HANDSHAKE_TIMEOUT",
        16 => "GROUP_KEY_UPDATE_TIMEOUT",
        17 => "IE_IN_4WAY_DIFFERS",
        18 => "GROUP_CIPHER_INVALID",
        19 => "PAIRWISE_CIPHER_INVALID",
        20 => "AKMP_INVALID",
        21 => "UNSUPP_RSN_IE_VERSION",
        22 => "INVALID_RSN_IE_CAP",
        23 => "IEEE_802_1X_AUTH_FAILED",
        24 => "CIPHER_SUITE_REJECTED",
        200 => "BEACON_TIMEOUT",
        201 => "NO_AP_FOUND",
        202 => "AUTH_FAIL",
        203 => "ASSOC_FAIL",
        204 => "HANDSHAKE_TIMEOUT",
        205 => "CONNECTION_FAIL",
        206 => "AP_TSF_RESET",
        207 => "ROAMING",
        _ => "UNKNOWN",
    }
}

// EOF
