// measure.rs — Radio reception + wMBus data pipeline

use crate::*;

// Radio watchdog timeout: restart if no packet in set time
const RADIO_WAIT_SECS: u64 = 600;

pub async fn read_meter(state: Arc<Pin<Box<MyState>>>, mut radio: Cc1101Radio<'_>) -> AppResult<()> {
    loop {
        if *state.net_up.read().await {
            break;
        }
        sleep(Duration::from_secs(5)).await;
    }
    info!("Network is up.");

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
        match radio.wait_for_packet(RADIO_WAIT_SECS).await? {
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
                warn!("No packets received in {RADIO_WAIT_SECS} s, restarting radio...");
                radio.restart_radio()?;
            }
        }
    }
}
// EOF
