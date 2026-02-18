// mqtt.rs

use esp_idf_svc::mqtt::{self, client::MessageId};
use esp_idf_sys::EspError;

use crate::*;

#[allow(unreachable_code)]
pub async fn run_mqtt(state: Arc<Pin<Box<MyState>>>) -> anyhow::Result<()> {
    if !state.config.read().await.mqtt_enable {
        info!("MQTT is disabled.");
        // we cannot return, otherwise tokio::select in main() will exit
        loop {
            sleep(Duration::from_secs(3600)).await;
        }
    }

    loop {
        if *state.wifi_up.read().await {
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }

    let url = state.config.read().await.mqtt_url.clone();
    let myid = state.myid.read().await.clone();

    sleep(Duration::from_secs(10)).await;

    info!("MQTT conn: {url} [{myid}]");
    let (client, conn) = match mqtt::client::EspAsyncMqttClient::new(
        &url,
        &mqtt::client::MqttClientConfiguration {
            client_id: Some(&myid),
            keep_alive_interval: Some(Duration::from_secs(25)),
            ..Default::default()
        },
    ) {
        Ok(c) => c,
        Err(e) => {
            let emsg = format!("MQTT conn failed: {e:?}");
            error!("{emsg}");
            bail!("{emsg}");
        }
    };

    tokio::select! {
        _ = Box::pin(data_sender(state.clone(), client)) => { error!("data_sender() ended."); }
        _ = Box::pin(event_loop(state.clone(), conn)) => { error!("event_loop() ended."); }
    };
    Ok(())
}

async fn data_sender(
    state: Arc<Pin<Box<MyState>>>,
    mut client: mqtt::client::EspAsyncMqttClient,
) -> anyhow::Result<()> {
    let mqtt_topic = state.config.read().await.mqtt_topic.clone();

    loop {
        sleep(Duration::from_secs(5)).await;
        let uptime = *(state.uptime.read().await);

        {
            let mut fresh_data = state.data_updated.write().await;
            if !*fresh_data {
                continue;
            }
            *fresh_data = false;
        }

        {
            let topic = format!("{mqtt_topic}/uptime");
            let mqtt_data = format!("{{ \"uptime\": {} }}", uptime);
            Box::pin(mqtt_send(&mut client, &topic, false, &mqtt_data)).await?;
        }

        // Publish meter reading if available
        if let Some(ref reading) = *state.meter.read().await {
            let topic = format!("{mqtt_topic}/meter");
            let mqtt_data = format!(
                "{{ \"total_m3\": {:.3}, \"target_m3\": {:.3}, \"flow_temp\": {}, \"ambient_temp\": {}, \"info_codes\": {}, \"uptime\": {} }}",
                reading.total_volume_l as f64 / 1000.0,
                reading.target_volume_l as f64 / 1000.0,
                reading.flow_temp,
                reading.ambient_temp,
                reading.info_codes,
                uptime
            );
            Box::pin(mqtt_send(&mut client, &topic, true, &mqtt_data)).await?;
        }
    }
}

async fn mqtt_send(
    client: &mut mqtt::client::EspAsyncMqttClient,
    topic: &str,
    retain: bool,
    data: &str,
) -> Result<MessageId, EspError> {
    info!("MQTT sending {topic} {data}");

    let result = client
        .publish(
            topic,
            mqtt::client::QoS::AtLeastOnce,
            retain,
            data.as_bytes(),
        )
        .await;
    if let Err(e) = result {
        let msg = format!("MQTT send error: {e}");
        error!("{msg}");
    }
    result
}

async fn event_loop(
    _state: Arc<Pin<Box<MyState>>>,
    mut conn: mqtt::client::EspAsyncMqttConnection,
) -> anyhow::Result<()> {
    while let Ok(notification) = Box::pin(conn.next()).await {
        info!("MQTT received: {:?}", notification.payload());
    }

    error!("MQTT connection closed.");
    Ok(())
}
// EOF
