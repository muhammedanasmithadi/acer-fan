#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo "This script must be run as root. Use: sudo $0" >&2
    exit 1
fi

systemctl disable --now acer-fan.service 2>/dev/null || true
rm -f /etc/systemd/system/acer-fan.service
rm -f /usr/local/bin/acer-fand /usr/local/bin/fan-mode /usr/local/bin/fan-speed
rm -f /etc/modules-load.d/acer-fan.conf
rm -f /etc/modprobe.d/acer-fan.conf
rm -f /etc/udev/rules.d/90-acer-fan.rules
rm -rf /usr/src/acer-fan-1.0.0
dkms remove acer-fan/1.0.0 --all 2>/dev/null || true
modprobe -r acer_fan 2>/dev/null || true
depmod -a
rm -f /etc/acer-fan.conf
rm -rf /run/acer-fan

systemctl daemon-reload
echo "acer-fan uninstalled."
