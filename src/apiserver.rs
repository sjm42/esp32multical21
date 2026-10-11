// apiserver.rs

use std::sync::atomic::Ordering;

use anyhow::Context;
use axum::{
    Json, Router,
    body::Body,
    extract::{
        DefaultBodyLimit, Form, State,
        rejection::{FormRejection, JsonRejection},
    },
    http::{Response, StatusCode, header},
    response::{Html, IntoResponse},
    routing::*,
};
pub use axum_macros::debug_handler;
use embedded_svc::http::client::Client as HttpClient;
use esp_idf_svc::http::client::Configuration as HttpConfiguration;

use crate::*;

const HTTP_BODY_LIMIT: usize = 8 * 1024;
const REBOOT_DELAY: Duration = Duration::from_secs(2);

type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    BadGateway(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self::BadGateway(message.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response<Body> {
        error!("HTTP handler failed: {self:#}");

        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::BadGateway(message) => (StatusCode::BAD_GATEWAY, message),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string()),
        };

        (status, Json(serde_json::json!({"ok": false, "message": message}))).into_response()
    }
}

fn schedule_reboot(state: Arc<Pin<Box<MyState>>>) {
    tokio::spawn(async move {
        sleep(REBOOT_DELAY).await;
        *state.reset.write().await = true;
    });
}

macro_rules! static_handler {
    ($fn_name:ident, $path:literal, $content_type:literal, $bytes:expr) => {
        async fn $fn_name(State(state): State<Arc<Pin<Box<MyState>>>>) -> Response<Body> {
            let cnt = state.api_cnt.fetch_add(1, Ordering::Relaxed);
            info!("#{cnt} {}", $path);

            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, $content_type),
                    (header::CONTENT_ENCODING, "gzip"),
                ],
                $bytes.to_vec(),
            )
                .into_response()
        }
    };
}

static_handler!(
    get_favicon,
    "/favicon.ico",
    "image/vnd.microsoft.icon",
    include_bytes!(concat!(env!("OUT_DIR"), "/favicon.ico.gz"))
);
static_handler!(
    get_form_js,
    "/form.js",
    "text/javascript; charset=utf-8",
    include_bytes!(concat!(env!("OUT_DIR"), "/form.js.gz"))
);
static_handler!(
    get_index_css,
    "/index.css",
    "text/css; charset=utf-8",
    include_bytes!(concat!(env!("OUT_DIR"), "/index.css.gz"))
);

pub async fn run_api_server(state: Arc<Pin<Box<MyState>>>) -> AppResult<()> {
    loop {
        if *state.net_up.read().await {
            break;
        }
        sleep(Duration::from_secs(5)).await;
    }

    let listen = format!("0.0.0.0:{}", HTTP_API_PORT);
    let addr = listen.parse::<net::SocketAddr>()?;

    let app = Router::new()
        .route("/", get(get_index))
        .route("/favicon.ico", get(get_favicon))
        .route("/form.js", get(get_form_js))
        .route("/index.css", get(get_index_css))
        .route("/uptime", get(get_uptime))
        .route("/conf", get(get_conf).post(set_conf).options(options))
        .route("/meter", get(get_meter))
        .route("/reset_conf", get(reset_conf))
        .route("/fw", post(update_fw).options(options))
        .layer(DefaultBodyLimit::max(HTTP_BODY_LIMIT))
        .with_state(state);
    // .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("API server listening to {listen}");
    Ok(axum::serve(listener, app.into_make_service()).await?)
}

pub async fn options(State(state): State<Arc<Pin<Box<MyState>>>>) -> Response<Body> {
    let cnt = state.api_cnt.fetch_add(1, Ordering::Relaxed);
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

async fn get_index(State(state): State<Arc<Pin<Box<MyState>>>>) -> ApiResult<Response<Body>> {
    let cnt = state.api_cnt.fetch_add(1, Ordering::Relaxed);
    info!("#{cnt} get_index()");

    let ota_slot = state.ota_slot.clone();
    let config = state.config.read().await.clone();
    let value_tuple: (&str, &dyn Any) = ("ota_slot", &ota_slot);
    let index = config
        .render_with_values(&value_tuple)
        .context("could not render index template")?;
    Ok((StatusCode::OK, Html(index)).into_response())
}

pub async fn get_uptime(State(state): State<Arc<Pin<Box<MyState>>>>) -> (StatusCode, Json<Uptime>) {
    let cnt = state.api_cnt.fetch_add(1, Ordering::Relaxed);
    info!("#{cnt} get_uptime()");

    let uptime = *state.uptime.read().await;
    (StatusCode::OK, Json(Uptime { uptime }))
}

pub async fn get_conf(State(state): State<Arc<Pin<Box<MyState>>>>) -> Response<Body> {
    let cnt = state.api_cnt.fetch_add(1, Ordering::Relaxed);
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
    let cnt = state.api_cnt.fetch_add(1, Ordering::Relaxed);
    info!("#{cnt} get_meter()");

    match &*state.latest_data.read().await {
        Some(reading) => (StatusCode::OK, Json(reading.clone())).into_response(),
        None => (StatusCode::OK, Json(serde_json::json!({"status": "no reading"}))).into_response(),
    }
}

async fn set_conf(
    State(state): State<Arc<Pin<Box<MyState>>>>,
    config_payload: Result<Json<MyConfig>, JsonRejection>,
) -> ApiResult<Response<Body>> {
    let cnt = state.api_cnt.fetch_add(1, Ordering::Relaxed);
    info!("#{cnt} set_conf()");

    let Json(mut config) =
        config_payload.map_err(|error| ApiError::bad_request(format!("Invalid config JSON: {error}")))?;

    if config.v4mask > 30 {
        return Err(ApiError::bad_request("IPv4 mask error: bits must be between 0..30"));
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

async fn reset_conf(State(state): State<Arc<Pin<Box<MyState>>>>) -> ApiResult<Response<Body>> {
    let cnt = state.api_cnt.fetch_add(1, Ordering::Relaxed);
    info!("#{cnt} reset_conf()");

    info!("Saving  default config to nvs...");
    Box::pin(save_conf(state, MyConfig::default())).await
}

async fn save_conf(state: Arc<Pin<Box<MyState>>>, config: MyConfig) -> ApiResult<Response<Body>> {
    let mut nvs = state.nvs.write().await;
    config.to_nvs(&mut nvs).context("could not save configuration to NVS")?;
    drop(nvs);

    info!("Config saved to nvs. Resetting soon...");
    schedule_reboot(state);
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({"ok": true, "message": "Config saved, rebooting"})),
    )
        .into_response())
}
async fn update_fw(
    State(state): State<Arc<Pin<Box<MyState>>>>,
    fw_update: Result<Form<UpdateFirmware>, FormRejection>,
) -> ApiResult<Response<Body>> {
    let cnt = state.api_cnt.fetch_add(1, Ordering::Relaxed);
    info!("#{cnt} update_fw()");

    let Form(fw_update) =
        fw_update.map_err(|error| ApiError::bad_request(format!("Invalid firmware request: {error}")))?;
    info!("Firmware update: \n{fw_update:#?}");
    if !fw_update.url.starts_with("http://") {
        return Err(ApiError::bad_request("Firmware URL must start with http://"));
    }

    let client_config = HttpConfiguration::default();

    let mut ota = EspOta::new().context("could not acquire OTA service")?;
    let connection = EspHttpConnection::new(&client_config).context("could not create firmware HTTP connection")?;
    let mut client = HttpClient::wrap(connection);
    let req = client
        .get(fw_update.url.as_str())
        .map_err(|error| ApiError::bad_request(format!("Invalid firmware URL: {error}")))?;
    let resp = req
        .submit()
        .map_err(|error| ApiError::bad_gateway(format!("Firmware download failed: {error}")))?;
    if resp.status() != 200 {
        return Err(ApiError::bad_gateway(format!(
            "Firmware download failed: HTTP {}",
            resp.status()
        )));
    }

    let mut update = ota.initiate_update().context("could not initiate OTA update")?;
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(8192)
        .context("could not allocate OTA transfer buffer")?;
    buffer.resize(8192, 0_u8);

    io::utils::copy(resp, &mut update, &mut buffer).map_err(|error| match error {
        io::utils::CopyError::Read(error) => {
            ApiError::bad_gateway(format!("Firmware download failed while reading: {error}"))
        }
        io::utils::CopyError::Write(error) => {
            ApiError::Internal(anyhow::anyhow!("firmware flash write failed: {error}"))
        }
    })?;
    update.complete().context("firmware validation or activation failed")?;

    info!("Update done. Restarting soon...");
    schedule_reboot(state);

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({"ok": true, "message": "Firmware updated, rebooting"})),
    )
        .into_response())
}
// EOF
