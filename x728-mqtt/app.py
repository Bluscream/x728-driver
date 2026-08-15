import json
import os
import time
import paho.mqtt.client as mqtt

MQTT_BROKER = os.getenv("MQTT_SERVER", os.getenv("MQTT_BROKER", "192.168.2.4"))
MQTT_PORT = int(os.getenv("MQTT_PORT", 1883))
MQTT_USER = os.getenv("MQTT_USER", "blu-pc")
MQTT_PASS = os.getenv("MQTT_PW", os.getenv("MQTT_PASS", "rpTekYYcZDdvsrDE8bWQXUWqr6nbSta5"))

SHM_PATH = "/dev/shm/x728_state.json"
CMD_PATH = "/dev/shm/x728_cmd.json"

DISCOVERY_PREFIX = "homeassistant"
DEVICE_ID = "x728_ups"

def on_connect(client, userdata, flags, rc, properties=None):
    print(f"[x728-mqtt] Connected to MQTT broker with result code: {rc}")
    if rc == 0 or rc.value == 0:
        publish_discovery(client)
        client.subscribe(f"{DEVICE_ID}/buzzer/set")
        client.subscribe(f"{DEVICE_ID}/reboot/set")
        client.subscribe(f"{DEVICE_ID}/shutdown/set")

def on_message(client, userdata, msg):
    payload = msg.payload.decode("utf-8").strip().upper()
    print(f"[x728-mqtt] Received MQTT command on {msg.topic}: {payload}")
    if msg.topic == f"{DEVICE_ID}/buzzer/set":
        active = (payload in ["ON", "TRUE", "1"])
        write_cmd({"buzzer": active})
    elif msg.topic in [f"{DEVICE_ID}/reboot/set", f"{DEVICE_ID}/reboot/press"]:
        write_cmd({"reboot": True})
    elif msg.topic in [f"{DEVICE_ID}/shutdown/set", f"{DEVICE_ID}/shutdown/press"]:
        write_cmd({"shutdown": True})

def write_cmd(cmd_dict):
    try:
        with open(CMD_PATH, "w", encoding="utf-8") as f:
            json.dump(cmd_dict, f)
        print(f"[x728-mqtt] Wrote command to {CMD_PATH}: {cmd_dict}")
    except Exception as err:
        print(f"[x728-mqtt] Failed to write command file: {err}")

def publish_discovery(client):
    device_info = {
        "identifiers": [DEVICE_ID],
        "name": "X728 UPS",
        "model": "X728 v2.5",
        "manufacturer": "Geekworm"
    }
    sensors = [
        ("voltage", "Voltage", "voltage", "V", "sensor"),
        ("capacity", "Battery Level", "battery", "%", "sensor"),
        ("state", "State", None, None, "sensor"),
        ("ac_power", "AC Power", "power", None, "binary_sensor"),
        ("button_pressed", "Power Button", "occupancy", None, "binary_sensor"),
        ("button_last_pressed_ms", "Button Last Pressed Timestamp", None, None, "sensor"),
    ]
    for key, name, device_class, unit, component in sensors:
        config_topic = f"{DISCOVERY_PREFIX}/{component}/{DEVICE_ID}/{key}/config"
        payload = {
            "name": name,
            "unique_id": f"{DEVICE_ID}_{key}",
            "state_topic": f"{DEVICE_ID}/{key}/state",
            "device": device_info
        }
        if device_class:
            payload["device_class"] = device_class
        if unit:
            payload["unit_of_measurement"] = unit
        client.publish(config_topic, json.dumps(payload), retain=True)

    # Discovery for Buzzer Switch
    switch_topic = f"{DISCOVERY_PREFIX}/switch/{DEVICE_ID}/buzzer/config"
    switch_payload = {
        "name": "Buzzer",
        "unique_id": f"{DEVICE_ID}_buzzer",
        "state_topic": f"{DEVICE_ID}/buzzer/state",
        "command_topic": f"{DEVICE_ID}/buzzer/set",
        "icon": "mdi:volume-high",
        "device": device_info
    }
    client.publish(switch_topic, json.dumps(switch_payload), retain=True)

    # Discovery for Reboot Button
    reboot_topic = f"{DISCOVERY_PREFIX}/button/{DEVICE_ID}/reboot/config"
    reboot_payload = {
        "name": "Reboot",
        "unique_id": f"{DEVICE_ID}_reboot",
        "command_topic": f"{DEVICE_ID}/reboot/set",
        "device_class": "restart",
        "icon": "mdi:restart",
        "device": device_info
    }
    client.publish(reboot_topic, json.dumps(reboot_payload), retain=True)

    # Discovery for Shutdown Button
    shutdown_topic = f"{DISCOVERY_PREFIX}/button/{DEVICE_ID}/shutdown/config"
    shutdown_payload = {
        "name": "Shutdown",
        "unique_id": f"{DEVICE_ID}_shutdown",
        "command_topic": f"{DEVICE_ID}/shutdown/set",
        "icon": "mdi:power",
        "device": device_info
    }
    client.publish(shutdown_topic, json.dumps(shutdown_payload), retain=True)

    print("[x728-mqtt] Home Assistant discovery entities (including Reboot & Shutdown buttons) published to MQTT.")

try:
    client = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2, client_id="x728_mqtt_bridge")
except AttributeError:
    client = mqtt.Client(client_id="x728_mqtt_bridge")

client.on_connect = on_connect
client.on_message = on_message

if MQTT_USER and MQTT_PASS:
    client.username_pw_set(MQTT_USER, MQTT_PASS)

print(f"[x728-mqtt] Connecting to MQTT broker at {MQTT_BROKER}:{MQTT_PORT} as user {MQTT_USER}...")
try:
    client.connect(MQTT_BROKER, MQTT_PORT, 60)
    client.loop_start()
except Exception as err:
    print(f"[x728-mqtt] Failed to connect to MQTT broker: {err}")

last_published_button_ts = 0

while True:
    if os.path.exists(SHM_PATH):
        try:
            with open(SHM_PATH, "r", encoding="utf-8") as f:
                data = json.load(f)
                
            voltage = data.get("voltage", 0)
            capacity = data.get("capacity", 0)
            ac_power = data.get("ac_power", False)

            if capacity <= 0.0:
                battery_state = "Empty"
            elif capacity >= 100.0:
                battery_state = "Full"
            elif ac_power:
                battery_state = "Charging"
            else:
                battery_state = "Discharging"

            client.publish(f"{DEVICE_ID}/voltage/state", f"{voltage:.2f}", retain=True)
            client.publish(f"{DEVICE_ID}/capacity/state", f"{capacity:.1f}", retain=True)
            client.publish(f"{DEVICE_ID}/state/state", battery_state, retain=True)
            client.publish(f"{DEVICE_ID}/ac_power/state", "ON" if ac_power else "OFF", retain=True)
            client.publish(f"{DEVICE_ID}/buzzer/state", "ON" if data.get("buzzer_active") else "OFF", retain=True)
            
            button_ts = data.get("button_last_pressed_ms", 0)
            client.publish(f"{DEVICE_ID}/button_last_pressed_ms/state", str(button_ts), retain=True)
            
            if data.get("button_pressed") or button_ts > last_published_button_ts:
                client.publish(f"{DEVICE_ID}/button_pressed/state", "ON", retain=True)
                if button_ts > last_published_button_ts:
                    last_published_button_ts = button_ts
            else:
                client.publish(f"{DEVICE_ID}/button_pressed/state", "OFF", retain=True)

        except Exception as err:
            print(f"[x728-mqtt] Error reading shared memory: {err}")
    time.sleep(1)
