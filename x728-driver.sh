#!/bin/sh
# X728 Container Driver Script for Network UPS Tools (NUT)
DEV_FILE="/etc/nut/x728.dev"
CMD_FILE="/etc/nut/x728.dev.cmd"
BUZZER_STATE="disabled"

read_i2c_telemetry() {
  python3 -c "
import smbus, struct
try:
    bus = smbus.SMBus(1)
    read_v = bus.read_word_data(0x36, 0x02)
    swapped_v = struct.unpack('<H', struct.pack('>H', read_v))[0]
    volt = (swapped_v * 1.25 / 1000.0 / 16.0)

    read_c = bus.read_word_data(0x36, 0x04)
    swapped_c = struct.unpack('<H', struct.pack('>H', read_c))[0]
    cap = (swapped_c / 256.0)

    print('%s:%s' % (round(volt, 2), round(cap, 1)))
except Exception as e:
    print('4.10:100.0')
" 2>/dev/null
}

read_ac_power() {
  if command -v gpioget >/dev/null 2>&1; then
    RAW=$(gpioget -c 0 6 2>/dev/null || gpioget 0 6 2>/dev/null)
    case "$RAW" in
      *inactive*|*0*) echo "0" ;;
      *) echo "1" ;;
    esac
  elif [ -f /sys/class/gpio/gpio6/value ]; then
    cat /sys/class/gpio/gpio6/value 2>/dev/null
  else
    echo "1"
  fi
}

read_cpu_temp() {
  if [ -f /sys/class/thermal/thermal_zone0/temp ]; then
    MILLIC=$(cat /sys/class/thermal/thermal_zone0/temp 2>/dev/null)
    python3 -c "print(round($MILLIC / 1000.0, 1))" 2>/dev/null
  else
    echo "45.0"
  fi
}

set_buzzer() {
  STATE="$1"
  if [ "$STATE" = "enabled" ]; then
    BUZZER_STATE="enabled"
    gpioset 0 20=1 2>/dev/null || true
  else
    BUZZER_STATE="disabled"
    gpioset 0 20=0 2>/dev/null || true
  fi
}

update_dev_file() {
  TELEM=$(read_i2c_telemetry)
  VOLT=$(echo "$TELEM" | cut -d':' -f1)
  CAP=$(echo "$TELEM" | cut -d':' -f2)
  AC_VAL=$(read_ac_power)
  TEMP=$(read_cpu_temp)

  if [ "$AC_VAL" = "0" ]; then
    STATUS="OB DISCHRG"
    IN_VOLT="0.0"
  else
    STATUS="OL"
    IN_VOLT="5.0"
  fi

  cat << EOF2 > "$DEV_FILE"
ambient.temperature: ${TEMP:-45.0}
battery.charge: ${CAP:-100.0}
battery.voltage: ${VOLT:-4.10}
battery.voltage.nominal: 3.7
battery.voltage.high: 4.20
battery.voltage.low: 3.00
device.mfr: Geekworm
device.model: X728 Integrated Container Driver
device.type: ups
driver.name: dummy-ups
driver.version: 2.8.3
input.voltage: ${IN_VOLT}
input.voltage.nominal: 5.0
output.voltage: 5.0
output.voltage.nominal: 5.0
ups.beeper.status: ${BUZZER_STATE}
ups.status: ${STATUS}
EOF2
  chmod 666 "$DEV_FILE" 2>/dev/null || true
}

while true; do
  update_dev_file

  if [ -f "$CMD_FILE" ]; then
    CMD=$(cat "$CMD_FILE")
    rm -f "$CMD_FILE"
    if [ "$CMD" = "shutdown" ] || [ "$CMD" = "FSD" ]; then
      poweroff || shutdown -h now
    elif [ "$CMD" = "reboot" ]; then
      reboot
    elif [ "$CMD" = "beeper.enable" ] || [ "$CMD" = "beeper.on" ] || [ "$CMD" = "beeper.toggle" ]; then
      set_buzzer "enabled"
    elif [ "$CMD" = "beeper.disable" ] || [ "$CMD" = "beeper.off" ]; then
      set_buzzer "disabled"
    fi
  fi

  sleep 2
done
