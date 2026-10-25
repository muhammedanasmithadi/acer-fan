#!/bin/sh
# Validate the acer_fan driver + acer-fand stack.
# Requires root. Exits nonzero on the first failed check.

set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo "This script must be run as root. Use: sudo $0" >&2
    exit 1
fi

pass=0
check() {
    desc="$1"
    shift
    if "$@" > /dev/null 2>&1; then
        echo "PASS: $desc"
        pass=$((pass + 1))
    else
        echo "FAIL: $desc"
        exit 1
    fi
}

check_value() {
    if [ "$2" = "$3" ]; then
        echo "PASS: $1 ($3)"
        pass=$((pass + 1))
    else
        echo "FAIL: $1 (expected '$2', got '$3')"
        exit 1
    fi
}

echo "== kernel module =="
check "acer_fan loaded" sh -c 'lsmod | grep -q "^acer_fan "'
if lsmod | grep -Eq "^(tuxedo_keyboard|clevo_wmi|clevo_acpi|tuxedo_io|tuxedo_nb|uniwill_wmi|ite_829)[[:space:]]"; then
    echo "FAIL: stale tuxedo/clevo modules still loaded"
    exit 1
fi
echo "PASS: no stale tuxedo/clevo modules loaded"
pass=$((pass + 1))

echo "== fan device =="
DEV=/sys/devices/platform/acer_fan
check "fan1_temp exists" test -r "$DEV/fan1_temp"
check "fan1_speed exists" test -w "$DEV/fan1_speed"
check "fan2_temp exists" test -r "$DEV/fan2_temp"
check "fan2_speed exists" test -w "$DEV/fan2_speed"
check "fan_mode exists" test -w "$DEV/fan_mode"

MAX=$(cat "$DEV/fan_max_speed")
MIN=$(cat "$DEV/fan_min_speed")
check_value "fan_min_speed is 20" "20" "$MIN"
check_value "fan_max_speed is 100" "100" "$MAX"

echo "== daemon =="
check "acer-fand active" systemctl is-active --quiet acer-fan.service
check "cmd file exists" test -f /run/acer-fan/cmd
if journalctl -u acer-fan --no-pager --since "2 min ago" 2>/dev/null | grep -qi "error"; then
    echo "FAIL: acer-fand logged errors in the last 2 minutes"
    exit 1
fi
echo "PASS: no acer-fand errors in the last 2 minutes"
pass=$((pass + 1))

echo "== live fan control =="
printf 'set manual 30\n' > /run/acer-fan/cmd
sleep 1
SPEED=$(cat "$DEV/fan1_speed")
if [ "$SPEED" -ge 25 ] && [ "$SPEED" -le 35 ]; then
    echo "PASS: manual fan speed accepted ($SPEED)"
    pass=$((pass + 1))
else
    echo "FAIL: manual fan speed mismatch ($SPEED)"
    exit 1
fi
printf 'auto\n' > /run/acer-fan/cmd
sleep 1
MODE=$(cat "$DEV/fan_mode")
check_value "fan mode is manual for daemon auto control" "manual" "$MODE"

echo ""
echo "All $pass checks passed."
