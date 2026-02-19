// measure.rs — Radio reception + wMBus data pipeline

use crate::*;

pub async fn read_meter(state: Arc<Pin<Box<MyState>>>, mut radio: Cc1101Radio<'_>) -> AppResult<()> {
    let mut cnt = 0;
    let ntp = sntp::EspSntp::new_default()?;
    sleep(Duration::from_secs(10)).await;

    loop {
        if *state.wifi_up.read().await {
            break;
        }

        if cnt > 300 {
            esp_idf_hal::reset::restart();
        }
        cnt += 1;
        sleep(Duration::from_millis(200)).await;
    }
    info!("WiFi connected.");

    cnt = 0;
    loop {
        if Utc::now().year() > 2020 && ntp.get_sync_status() == sntp::SyncStatus::Completed {
            break;
        }

        if cnt > 300 {
            esp_idf_hal::reset::restart();
        }
        cnt += 1;
        sleep(Duration::from_millis(200)).await;
    }
    info!("NTP ok.");

    // Parse meter config
    let (meter_id, meter_key) = {
        let config = state.config.read().await;
        match (config.meter_id_bytes(), config.meter_key_bytes()) {
            (Some(id), Some(key)) => (id, key),
            _ => {
                warn!("No valid meter_id and/or meter_key configured.");
                error!("Now we are doing nothing useful. Radio is idle.");
                loop {
                    sleep(Duration::from_secs(3600)).await;
                }
            }
        }
    };

    info!(
        "Meter ID: {:02X}{:02X}{:02X}{:02X}, key configured. Initializing radio...",
        meter_id[0], meter_id[1], meter_id[2], meter_id[3]
    );

    radio.init()?;

    info!("Waiting for wMBus packets...");
    loop {
        match radio.wait_for_packet().await? {
            Some(payload) => {
                info!("Got wMBus packet ({} bytes), parsing...", payload.len());
                match parse_frame(&payload, &meter_id, &meter_key) {
                    Some(reading) => {
                        info!("Meter reading: {:?}", reading);
                        *state.latest_data.write().await = Some(reading);
                        *state.data_updated.write().await = true;
                    }
                    None => {
                        info!("Packet did not yield a valid reading");
                    }
                }
            }
            None => {
                // Watchdog timeout, restart radio
                radio.restart_radio()?;
            }
        }
    }
}
// EOF
