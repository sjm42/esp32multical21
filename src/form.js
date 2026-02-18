var postCfgDataAsJson = async ({
                                   url, formData
                               }) => {
    const formObj = Object.fromEntries(formData.entries());
    formObj.port = parseInt(formObj.port);
    formObj.retries = parseInt(formObj.retries);
    formObj.delay = parseInt(formObj.delay);
    formObj.v4dhcp = (formObj.v4dhcp === "on");
    formObj.v4mask = parseInt(formObj.v4mask);
    formObj.mqtt_enable = (formObj.mqtt_enable === "on");
    if (!formObj.meter_id) formObj.meter_id = "";
    if (!formObj.meter_key) formObj.meter_key = "";
    const formDataJsonString = JSON.stringify(formObj);

    const fetchOptions = {
        method: "POST", mode: 'cors', keepalive: false, headers: {
            'Accept': 'application/json', 'Content-Type': 'application/json',
        }, body: formDataJsonString,
    };
    const response = await fetch(url, fetchOptions);

    if (!response.ok) {
        const errorMessage = await response.text();
        throw new Error(errorMessage);
    }

    return response.json();
}

var handleCfgSubmit = async (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    const url = form.action;

    try {
        formData = new FormData(form);
        const responseData = await postCfgDataAsJson({
            url, formData
        });
        console.log({
            responseData
        });
    } catch (error) {
        console.error(error);
    }
}

document.addEventListener("DOMContentLoaded", function () {
    document.querySelector("form[name='esp32cfg']")
        .addEventListener("submit", handleCfgSubmit);
});

async function update_uptime() {
    var o = document.getElementById("uptime");
    o.innerHTML = "Updating...";
    var url = "/uptime";
    const response = await fetch(url);
    const json = JSON.parse(await response.text());
    o.innerHTML = "<p>Uptime: " + json.uptime + " s</p>";
}

async function update_meter() {
    var o = document.getElementById("meter");
    try {
        const response = await fetch("/meter");
        const json = await response.json();
        if (json.total_volume_l !== undefined) {
            o.innerHTML = "<table>" +
                "<tr><td>Total:</td><td>" + (json.total_volume_l / 1000).toFixed(3) + " m3</td></tr>" +
                "<tr><td>Target:</td><td>" + (json.target_volume_l / 1000).toFixed(3) + " m3</td></tr>" +
                "<tr><td>Flow temp:</td><td>" + json.flow_temp + " &deg;C</td></tr>" +
                "<tr><td>Ambient temp:</td><td>" + json.ambient_temp + " &deg;C</td></tr>" +
                "<tr><td>Info codes:</td><td>0x" + json.info_codes.toString(16).padStart(2, '0') + "</td></tr>" +
                "<tr><td>Timestamp:</td><td>" + json.timestamp + "</td></tr>" +
                "</table>";
        } else {
            o.innerHTML = "<p>No meter reading yet</p>";
        }
    } catch (e) {
        o.innerHTML = "<p>Error fetching meter data</p>";
    }
}

function onLoad() {
    setInterval(update_uptime, 30e3);
    setInterval(update_meter, 30e3);
    update_meter();
}
