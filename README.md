# Geekworm X728 UPS Driver for Network UPS Tools (NUT) & Linux

Native Rust event-driven daemon and Network UPS Tools (NUT) container driver integration for the **Geekworm X728** UPS expansion board on Raspberry Pi.

## 🚀 Features

- **Direct I2C & GPIO Integration**: Reads MAX17048 fuel gauge (Voltage & Capacity %) and GPIO 6 AC loss detection.
- **Network UPS Tools (NUT) Driver**: Works natively with `nut-upsd` (e.g., `ghcr.io/tigattack/nut-upsd`) via NUT `dummy-ups` integration.
- **Home Assistant Ready**: Exposes live battery voltage, capacity, charging/discharging states, and power-off / reboot actions.
- **IPC Command Support**: JSON IPC interface for controlling hardware reboot, shutdown, and buzzer actions.

---

## 🛠️ Usage with Docker / Portainer

### 1. `ups.conf` configuration
Add the X728 UPS entry to your `/etc/nut/ups.conf`:

> ⚠️ **Important**: Set `mode = dummy-loop` so `dummy-ups` keeps the driver connection continuously open for Home Assistant.

```ini
[x728]
  driver = dummy-ups
  port = /etc/nut/x728.dev
  mode = dummy-loop
  pollinterval = 2
  desc = "Geekworm X728 Expansion Board"
```

### 2. Driver script (`x728-driver.sh`)
Place `x728-driver.sh` into your NUT container config folder (`/etc/nut/x728-driver.sh`).

### 3. Docker Run / Compose
Mount `/dev` and `/sys` into the container so it can access hardware I2C and GPIO:

```bash
docker run -d \
  --name nut-upsd \
  --restart=unless-stopped \
  --privileged \
  --net=host \
  --entrypoint /etc/nut/entrypoint-custom.sh \
  -v /dev:/dev \
  -v /sys:/sys \
  -v /etc/nut:/etc/nut \
  ghcr.io/tigattack/nut-upsd:latest
```

---

## 🔒 Home Assistant Integration

Connect Home Assistant to `nut-upsd` on port `3493`:
- **Host**: `<your-pi-ip>`
- **Port**: `3493`
- **UPS Name**: `x728`

Exposes:
- `battery.charge` (%)
- `battery.voltage` (V)
- `input.voltage` (5.0 V DC)
- `ups.status` (`OL` = On Line, `OB DISCHRG` = On Battery / Discharging)

---

## 📜 License
MIT License
