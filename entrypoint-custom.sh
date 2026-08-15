#!/bin/sh
# Custom Container Entrypoint for nut-upsd
if [ ! -d /sys/class/gpio/gpio6 ] && [ -w /sys/class/gpio/export ]; then
  echo 6 > /sys/class/gpio/export 2>/dev/null || true
fi

sh /etc/nut/x728-driver.sh >/dev/null 2>&1 &

exec /entrypoint.sh "$@"
