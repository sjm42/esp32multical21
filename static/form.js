// form.js for esp32multical21

document.addEventListener("DOMContentLoaded", function () {
    bindForm("esp32cfg", handleCfgSubmit);
    bindForm("esp32fw", handleFwSubmit);
    initUptime();
    initMeter();
});

function bindForm(name, handler) {
    const form = document.querySelector(`form[name='${name}']`);
    if (!form) return;
    ensureStatusNode(form);
    form.addEventListener("submit", handler);
}

function ensureStatusNode(form) {
    let status = form.querySelector(".form-status");
    if (status) return status;

    status = document.createElement("div");
    status.className = "form-status";
    status.setAttribute("role", "status");
    status.setAttribute("aria-live", "polite");
    status.hidden = true;
    form.appendChild(status);
    return status;
}

function setFormStatus(form, kind, message) {
    const status = ensureStatusNode(form);
    status.hidden = !message;
    status.className = `form-status${kind ? ` is-${kind}` : ""}`;
    status.textContent = message || "";
}

function setFormBusy(form, busy, busyLabel) {
    const submit = form.querySelector("input[type='submit']");
    if (!submit) return;

    if (busy) {
        if (!submit.dataset.label) submit.dataset.label = submit.value;
        submit.disabled = true;
        submit.value = busyLabel || "Working...";
    } else {
        submit.disabled = false;
        if (submit.dataset.label) submit.value = submit.dataset.label;
    }
}

async function fetchPayloadOrError(url, options) {
    const response = await fetch(url, options);
    const contentType = response.headers.get("content-type") || "";

    let payload;
    if (contentType.includes("application/json")) {
        payload = await response.json();
    } else {
        const text = await response.text();
        payload = {message: (text || "").trim()};
    }

    if (!response.ok || payload.ok === false) {
        throw new Error(payload.message || `Request failed (${response.status})`);
    }
    return payload;
}

async function updateUptime() {
    const node = document.getElementById("uptime");
    if (!node) return;

    try {
        const response = await fetch("/uptime");
        const json = await response.json();
        node.textContent = `Uptime: ${json.uptime} s`;
    } catch (_error) {
        node.textContent = "Uptime unavailable";
    }
}

function initUptime() {
    if (!document.getElementById("uptime")) return;
    updateUptime();
    window.setInterval(updateUptime, 30e3);
}

async function updateMeter() {
    const node = document.getElementById("meter");
    if (!node) return;

    try {
        const response = await fetch("/meter");
        const json = await response.json();
        if (json.total_m3 !== undefined) {
            node.innerHTML = "<table>" +
                "<tr><td>Total:</td><td>" + json.total_m3.toFixed(3) + " m3 (" + json.total_l + " l)</td></tr>" +
                "<tr><td>Month start:</td><td>" + json.month_start_m3.toFixed(3) + " m3 (" + json.month_start_l + " l)</td></tr>" +
                "<tr><td>Flow temp:</td><td>" + json.flow_temp + " &deg;C</td></tr>" +
                "<tr><td>Ambient temp:</td><td>" + json.ambient_temp + " &deg;C</td></tr>" +
                "<tr><td>Info codes:</td><td>0x" + json.info_codes.toString(16).padStart(2, "0") + "</td></tr>" +
                "<tr><td>Timestamp:</td><td>" + json.timestamp + "</td></tr>" +
                "<tr><td>Data received at:</td><td>" + json.timestamp_s + "</td></tr>" +
                "</table>";
        } else {
            node.innerHTML = "<p>No meter reading yet</p>";
        }
    } catch (_error) {
        node.innerHTML = "<p>Error fetching meter data</p>";
    }
}

function initMeter() {
    if (!document.getElementById("meter")) return;
    updateMeter();
    window.setInterval(updateMeter, 30e3);
}

const handleCfgSubmit = async (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    const url = form.action;

    setFormBusy(form, true, "Saving...");
    setFormStatus(form, "busy", "Saving config...");
    try {
        const formData = new FormData(form);
        const responseData = await postCfgDataAsJson({url, formData});
        setFormStatus(form, "ok", responseData.message || "Config saved, device will reboot");
    } catch (error) {
        console.error(error);
        setFormStatus(form, "error", error.message || "Config save failed");
    } finally {
        setFormBusy(form, false);
    }
};

const handleFwSubmit = async (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    const url = form.action;
    const firmwareUrl = String(new FormData(form).get("url") || "").trim();

    try {
        const parsed = new URL(firmwareUrl);
        if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
            throw new Error("Firmware URL must start with http:// or https://");
        }
    } catch (_error) {
        setFormStatus(form, "error", "Firmware URL must start with http:// or https://");
        return;
    }

    if (!window.confirm("Start firmware update now? The device will reboot if the update succeeds.")) {
        return;
    }

    setFormBusy(form, true, "Updating...");
    setFormStatus(form, "busy", "Downloading and flashing firmware...");
    try {
        const formData = new FormData(form);
        const responseData = await postFwForm({url, formData});
        setFormStatus(form, "ok", responseData.message || "Firmware update started");
    } catch (error) {
        console.error(error);
        setFormStatus(form, "error", error.message || "Firmware update failed");
    } finally {
        setFormBusy(form, false);
    }
};

const postCfgDataAsJson = async ({url, formData}) => {
    const formObj = Object.fromEntries(formData.entries());
    formObj.wifi_wpa2ent = (formObj.wifi_wpa2ent === "on");
    formObj.v4dhcp = (formObj.v4dhcp === "on");
    formObj.v4mask = parseInt(formObj.v4mask, 10);
    formObj.esphome_enable = (formObj.esphome_enable === "on");
    formObj.mqtt_enable = (formObj.mqtt_enable === "on");
    if (!formObj.wifi_username) formObj.wifi_username = "";
    if (!formObj.wifi_wpa2ent) formObj.wifi_username = "";
    if (!formObj.meter_id) formObj.meter_id = "";
    if (!formObj.meter_key) formObj.meter_key = "";

    return fetchPayloadOrError(url, {
        method: "POST",
        mode: "cors",
        keepalive: false,
        headers: {"Accept": "application/json", "Content-Type": "application/json"},
        body: JSON.stringify(formObj)
    });
};

const postFwForm = async ({url, formData}) => {
    const params = new URLSearchParams(formData);
    return fetchPayloadOrError(url, {
        method: "POST",
        body: params
    });
};

// EOF
