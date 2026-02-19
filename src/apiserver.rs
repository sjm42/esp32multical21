// apiserver.rs

use axum::{
    Json, Router,
    body::Body,
    extract::{Form, State, rejection::JsonRejection},
    http::{Response, StatusCode, header},
    response::{Html, IntoResponse},
    routing::*,
};
pub use axum_macros::debug_handler;
use embedded_svc::http::client::Client as HttpClient;

use crate::*;

pub async fn run_api_server(state: Arc<Pin<Box<MyState>>>) -> AppResult<()> {
    loop {
        if *state.wifi_up.read().await {
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }

    let listen = format!("0.0.0.0:{}", state.config.read().await.port);
    let addr = listen.parse::<net::SocketAddr>()?;

    let app = Router::new()
        .route("/", get(get_index))
        .route("/favicon.ico", get(get_favicon))
        .route("/form.js", get(get_formjs))
        .route("/uptime", get(get_uptime))
        .route("/conf", get(get_conf).post(set_conf).options(options))
        .route("/meter", get(get_meter))
        .route("/reset_conf", get(reset_conf))
        .route("/fw", post(update_fw).options(options))
        .with_state(state);
    // .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("API server listening to {listen}");
    Ok(axum::serve(listener, app.into_make_service()).await?)
}

pub async fn options(State(state): State<Arc<Pin<Box<MyState>>>>) -> Response<Body> {
    let cnt = {
        let mut c = state.api_cnt.write().await;
        *c += 1;
        *c
    };
    info!("#{cnt} options()");

    (
        StatusCode::OK,
        [
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::ACCESS_CONTROL_ALLOW_METHODS, "get,post"),
            (header::ACCESS_CONTROL_ALLOW_HEADERS, "content-type"),
        ],
    )
        .into_response()
}

pub async fn get_index(State(state): State<Arc<Pin<Box<MyState>>>>) -> Response<Body> {
    let cnt = {
        let mut c = state.api_cnt.write().await;
        *c += 1;
        *c
    };
    info!("#{cnt} get_index()");

    let ota_slot = state.ota_slot.clone();
    let config = state.config.read().await.clone();
    let value_tuple: (&str, &dyn Any) = ("ota_slot", &ota_slot);
    let index = match config.render_with_values(&value_tuple) {
        Err(e) => {
            let err_msg = format!("Index template error: {e:?}\n");
            error!("{err_msg}");
            return (StatusCode::INTERNAL_SERVER_ERROR, err_msg).into_response();
        }
        Ok(s) => s,
    };
    (StatusCode::OK, Html(index)).into_response()
}

pub async fn get_favicon(State(state): State<Arc<Pin<Box<MyState>>>>) -> Response<Body> {
    let cnt = {
        let mut c = state.api_cnt.write().await;
        *c += 1;
        *c
    };
    info!("#{cnt} get_favicon()");

    let favicon = include_bytes!("favicon.ico");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/vnd.microsoft.icon")],
        favicon.to_vec(),
    )
        .into_response()
}

pub async fn get_formjs(State(state): State<Arc<Pin<Box<MyState>>>>) -> Response<Body> {
    let cnt = {
        let mut c = state.api_cnt.write().await;
        *c += 1;
        *c
    };
    info!("#{cnt} get_formjs()");

    let formjs = include_bytes!("form.js");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/javascript")],
        formjs.to_vec(),
    )
        .into_response()
}

pub async fn get_uptime(State(state): State<Arc<Pin<Box<MyState>>>>) -> (StatusCode, Json<Uptime>) {
    let cnt = {
        let mut c = state.api_cnt.write().await;
        *c += 1;
        *c
    };
    info!("#{cnt} get_uptime()");

    let uptime = *state.uptime.read().await;
    (StatusCode::OK, Json(Uptime { uptime }))
}

pub async fn get_conf(State(state): State<Arc<Pin<Box<MyState>>>>) -> Response<Body> {
    let cnt = {
        let mut c = state.api_cnt.write().await;
        *c += 1;
        *c
    };
    info!("#{cnt} get_conf()");

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "config": state.config.read().await.clone(),
        })),
    )
        .into_response()
}

pub async fn get_meter(State(state): State<Arc<Pin<Box<MyState>>>>) -> Response<Body> {
    let cnt = {
        let mut c = state.api_cnt.write().await;
        *c += 1;
        *c
    };
    info!("#{cnt} get_meter()");

    match &*state.latest_data.read().await {
        Some(reading) => (StatusCode::OK, Json(reading.clone())).into_response(),
        None => (StatusCode::OK, Json(serde_json::json!({"status": "no reading"}))).into_response(),
    }
}

pub async fn set_conf(
    State(state): State<Arc<Pin<Box<MyState>>>>,
    config_payload: Result<Json<MyConfig>, JsonRejection>,
) -> Response<Body> {
    let cnt = {
        let mut c = state.api_cnt.write().await;
        *c += 1;
        *c
    };
    info!("#{cnt} set_conf()");

    let Json(mut config) = match config_payload {
        Ok(config) => config,
        Err(e) => {
            let msg = format!("Invalid config JSON: {e}");
            error!("{msg}");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"ok": false, "message": msg})),
            )
                .into_response();
        }
    };

    if config.v4mask > 30 {
        let msg = "IPv4 mask error: bits must be between 0..30";
        error!("{}", msg);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"ok": false, "message": msg})),
        )
            .into_response();
    }

    if !config.wifi_wpa2ent {
        // Username is only used for WPA2 Enterprise.
        config.wifi_username.clear();
    }

    if config.v4dhcp {
        // clear out these if we are using DHCP
        config.v4addr = net::Ipv4Addr::new(0, 0, 0, 0);
        config.v4mask = 0;
        config.v4gw = net::Ipv4Addr::new(0, 0, 0, 0);
        config.dns1 = net::Ipv4Addr::new(0, 0, 0, 0);
        config.dns2 = net::Ipv4Addr::new(0, 0, 0, 0);
    }

    info!("Saving new config to nvs...");
    Box::pin(save_conf(state, config)).await
}

pub async fn reset_conf(State(state): State<Arc<Pin<Box<MyState>>>>) -> Response<Body> {
    let cnt = {
        let mut c = state.api_cnt.write().await;
        *c += 1;
        *c
    };
    info!("#{cnt} reset_conf()");

    info!("Saving  default config to nvs...");
    Box::pin(save_conf(state, MyConfig::default())).await
}

async fn save_conf(state: Arc<Pin<Box<MyState>>>, config: MyConfig) -> Response<Body> {
    let mut nvs = state.nvs.write().await;
    match config.to_nvs(&mut nvs) {
        Ok(_) => {
            info!("Config saved to nvs. Resetting soon...");
            *state.reset.write().await = true;
            (
                StatusCode::OK,
                Json(serde_json::json!({"ok": true, "message": "Config saved, rebooting"})),
            )
                .into_response()
        }
        Err(e) => {
            let msg = format!("Nvs write error: {e:?}");
            error!("{}", msg);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"ok": false, "message": msg})),
            )
                .into_response()
        }
    }
}
async fn update_fw(
    State(state): State<Arc<Pin<Box<MyState>>>>,
    Form(fw_update): Form<UpdateFirmware>,
) -> Response<Body> {
    let cnt = {
        let mut c = state.api_cnt.write().await;
        *c += 1;
        *c
    };
    info!("#{cnt} update_fw()");

    info!("Firmware update: \n{fw_update:#?}");
    let url = fw_update.url.to_owned();

    let mut ota = EspOta::new().unwrap();
    let mut client = HttpClient::wrap(EspHttpConnection::new(&Default::default()).unwrap());
    let req = client.get(&url).unwrap();
    let resp = req.submit().unwrap();
    if resp.status() != 200 {
        let emsg = format!("Firmware download failed: HTTP {}", resp.status());
        error!("{emsg}");
        return (StatusCode::BAD_GATEWAY, emsg).into_response();
    }

    let update_src = Box::new(resp);
    let mut update = ota.initiate_update().unwrap();
    let mut buffer = [0_u8; 8192];

    io::utils::copy(update_src, &mut update, &mut buffer).unwrap();
    update.complete().unwrap();

    info!("Update done. Restarting...");
    esp_idf_svc::hal::reset::restart();
}
// EOF
