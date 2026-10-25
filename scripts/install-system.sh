#!/bin/sh
# Install acer-fand system-wide: daemon, scripts, config, systemd unit,
# and the acer_fan DKMS driver.
# Requires root privileges.

set -eu

if [ "$(id -u)" -ne 0 ]; then
    echo "This script must be run as root. Use: sudo $0" >&2
    exit 1
fi

# Prefer the invoking user's Rust toolchain when running under sudo.
if ! command -v cargo > /dev/null 2>&1 && [ -n "${SUDO_USER:-}" ]; then
    SUDO_HOME=$(getent passwd "$SUDO_USER" | cut -d: -f6)
    for cand in "$SUDO_HOME"/.rustup/toolchains/*/bin \
                "$SUDO_HOME/.cargo/bin" \
                "$SUDO_HOME/.local/share/mise/shims"; do
        if [ -x "$cand/cargo" ]; then
            PATH="$cand:$PATH"
            export PATH
            break
        fi
    done
    if [ -d "$SUDO_HOME/.rustup" ]; then
        RUSTUP_HOME="$SUDO_HOME/.rustup"
        export RUSTUP_HOME
    fi
    unset SUDO_HOME
fi

for tool in cargo dkms depmod modprobe systemctl install; do
    command -v "$tool" > /dev/null || {
        echo "missing required tool: $tool" >&2
        exit 1
    }
done

if [ ! -d "/lib/modules/$(uname -r)/build" ]; then
    echo "kernel headers missing for $(uname -r): sudo dnf install kernel-devel" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
echo "Installing from: $SCRIPT_DIR"

echo "  stopping existing acer-fan instances"
systemctl stop acer-fan.service 2>/dev/null || true
pkill -x acer-fand 2>/dev/null || true

echo "  building acer-fand..."
(cd "$SCRIPT_DIR" && cargo build --release)
if [ -n "${SUDO_USER:-}" ]; then
    chown -R "$SUDO_USER" "$SCRIPT_DIR/target" 2>/dev/null || true
fi

echo "  /usr/local/bin/acer-fand"
install -m755 "$SCRIPT_DIR"/target/release/acer-fand /usr/local/bin/acer-fand

echo "  /usr/local/bin/fan-mode fan-speed"
install -m755 "$SCRIPT_DIR/scripts/fan-mode" /usr/local/bin/fan-mode
install -m755 "$SCRIPT_DIR/scripts/fan-speed" /usr/local/bin/fan-speed

echo "  /etc/acer-fan.conf"
if [ ! -f /etc/acer-fan.conf ]; then
    printf 'mode=auto\nprofile=balanced\nmin_speed=20\nmax_speed=100\noffset=0\nmanual_speed=40\n' > /etc/acer-fan.conf
    chmod 644 /etc/acer-fan.conf
fi

echo "  modules-load.d / modprobe.d"
install -d -m755 /etc/modules-load.d /etc/modprobe.d
install -m644 "$SCRIPT_DIR/packaging/modules-load.d/acer-fan.conf" /etc/modules-load.d/acer-fan.conf

echo "  acer_fan DKMS driver"
DKMS_SRC=/usr/src/acer-fan-1.0.0
(cd "$SCRIPT_DIR/driver" && make clean > /dev/null 2>&1) || true
rm -rf "$DKMS_SRC"
cp -r "$SCRIPT_DIR/driver" "$DKMS_SRC"
dkms remove acer-fan/1.0.0 --all 2>/dev/null || true
dkms add "$DKMS_SRC"
dkms install acer-fan/1.0.0
depmod -a

# Do not leave tuxedo/clevo fan interfaces bound to the same ACPI device.
for m in tuxedo_io tuxedo_keyboard clevo_wmi clevo_acpi uniwill_wmi tuxedo_compatibility_check; do
    modprobe -r "$m" 2>/dev/null || true
done
rmmod acer_fan 2>/dev/null || true
modprobe acer_fan

echo "  acer-fan.service"
install -m644 "$SCRIPT_DIR/packaging/acer-fan.service" /etc/systemd/system/acer-fan.service

systemctl daemon-reload
systemctl enable --now acer-fan.service

echo ""
echo "Installation complete."
echo "  - acer_fan driver installed via DKMS and loaded"
echo "  - acer-fan.service enabled and started"
echo "  - fan-mode, fan-speed available in /usr/local/bin"
echo ""
echo "Verify:"
echo "  sudo $SCRIPT_DIR/scripts/validate-driver.sh"
