// bin/esp32multical21.rs

#![warn(clippy::large_futures)]

use esp_idf_svc::{eventloop::EspSystemEventLoop, ping, timer::EspTaskTimerService};
use esp_idf_sys::esp;
use esp32multical21::*;

const CONFIG_RESET_COUNT: i32 = 9;

// esp_app_desc!();

fn main() -> anyhow::Result<()> {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    #[allow(clippy::needless_update)]
    let config = esp_idf_sys::esp_vfs_eventfd_config_t {
        max_fds: 1,
        ..Default::default()
    };
    esp! { unsafe { esp_idf_sys::esp_vfs_eventfd_register(&config) } }?;

    info!("Hello.");
    info!("Starting up.");

    let sysloop = EspSystemEventLoop::take()?;
    let timer = EspTaskTimerService::new()?;
    let nvs_default_partition = nvs::EspDefaultNvsPartition::take()?;

    let ns = env!("CARGO_BIN_NAME");
    let mut nvs = match nvs::EspNvs::new(nvs_default_partition.clone(), ns, true) {
        Ok(nvs) => {
            info!("Got namespace {ns:?} from default partition");
            nvs
        }
        Err(e) => panic!("Could not get namespace {ns}: {e:?}"),
    };

    let config = match MyConfig::from_nvs(&mut nvs) {
        None => {
            error!("Could not read nvs config, using defaults");
            let c = MyConfig::default();
            c.to_nvs(&mut nvs)?;
            info!("Successfully saved default config to nvs.");
            c
        }
        Some(c) => c,
    };
    info!("My config:\n{config:#?}");

    let ota_slot = {
        let mut ota = EspOta::new()?;
        let running_slot = ota.get_running_slot()?;
        ota.mark_running_slot_valid()?;
        let slot = format!("{} ({:?})", &running_slot.label, running_slot.state);
        info!("OTA slot: {slot}");
        slot
    };

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;
    let button = PinDriver::input(pins.gpio9.downgrade_input())?;

    // SPI pins: GPIO4=SCK, GPIO6=MOSI, GPIO5=MISO, GPIO7=CS
    let driver = spi::SpiDriver::new(
        peripherals.spi2,
        pins.gpio4,
        pins.gpio6,
        Some(pins.gpio5),
        &spi::SpiDriverConfig::new(),
    )?;
    let spi_cfg = spi::config::Config::new().baudrate(4.MHz().into());
    let dev = spi::SpiDeviceDriver::new(&driver, Some(pins.gpio7), &spi_cfg)?;

    // GDO0 on GPIO10
    let gdo0 = PinDriver::input(pins.gpio10.downgrade_input())?;

    // Create CC1101 radio
    let radio = Cc1101Radio::new(dev, gdo0);

    let wifidriver = WifiDriver::new(peripherals.modem, sysloop.clone(), Some(nvs_default_partition))?;

    let state = Box::pin(MyState::new(config, nvs, ota_slot));
    let shared_state = Arc::new(state);

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(Box::pin(async move {
            let wifi_loop = WifiLoop {
                state: shared_state.clone(),
                wifi: None,
            };

            info!("Entering main loop...");
            tokio::select! {
                _ = Box::pin(poll_reset(shared_state.clone(), button)) => { error!("poll_reset() ended."); }
                _ = Box::pin(read_meter(shared_state.clone(), radio)) => { error!("poll_sensors() ended."); }
                _ = Box::pin(run_mqtt(shared_state.clone())) => { error!("run_mqtt() ended."); }
                _ = Box::pin(run_api_server(shared_state.clone())) => { error!("run_api_server() ended."); }
                _ = Box::pin(run_esphome_api(shared_state.clone())) => { error!("run_esphome_api() ended."); }
                _ = Box::pin(wifi_loop.run(wifidriver, sysloop, timer)) => { error!("wifi_loop.run() ended."); }
                _ = Box::pin(pinger(shared_state.clone())) => { error!("pinger() ended."); }
            };
        }));

    info!("main() finished, reboot.");
    FreeRtos::delay_ms(3000);
    esp_idf_hal::reset::restart();
}

async fn poll_reset(mut state: Arc<Pin<Box<MyState>>>, button: PinDriver<'_, AnyInputPin, Input>) -> AppResult<()> {
    let mut uptime: usize = 0;
    loop {
        sleep(Duration::from_secs(2)).await;

        uptime += 2;
        *(state.uptime.write().await) = uptime;

        if *state.reset.read().await {
            esp_idf_hal::reset::restart();
        }

        if button.is_low() {
            Box::pin(reset_button(&mut state, &button)).await?;
        }
    }
}

async fn reset_button<'a>(
    state: &mut Arc<std::pin::Pin<Box<MyState>>>,
    button: &PinDriver<'a, AnyInputPin, Input>,
) -> AppResult<()> {
    let mut reset_cnt = CONFIG_RESET_COUNT;

    while button.is_low() {
        let msg = format!("Reset? {reset_cnt}");
        error!("{msg}");

        if reset_cnt == 0 {
            error!("Factory resetting...");

            let new_config = MyConfig::default();
            new_config.to_nvs(&mut *state.nvs.write().await)?;
            sleep(Duration::from_millis(2000)).await;
            esp_idf_hal::reset::restart();
        }

        reset_cnt -= 1;
        sleep(Duration::from_millis(500)).await;
        continue;
    }
    Ok(())
}

async fn pinger(state: Arc<Pin<Box<MyState>>>) -> AppResult<()> {
    loop {
        sleep(Duration::from_secs(300)).await;

        if let Some(ping_ip) = *state.ping_ip.read().await {
            let if_idx = *state.if_index.read().await;
            if if_idx > 0 {
                info!("Starting ping {ping_ip} (if_idx {if_idx})");
                let conf = ping::Configuration {
                    count: 3,
                    interval: Duration::from_secs(1),
                    timeout: Duration::from_secs(1),
                    data_size: 64,
                    tos: 0,
                };
                let mut ping = ping::EspPing::new(if_idx);
                let res = ping.ping(ping_ip, &conf)?;
                info!("Pinger result: {res:?}");
                if res.received == 0 {
                    error!("Ping failed, rebooting.");
                    sleep(Duration::from_millis(2000)).await;
                    esp_idf_hal::reset::restart();
                }
            } else {
                error!("No if_index. wat?");
            }
        }
    }
}
// EOF
