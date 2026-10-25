# acer-fan

Fan control for the Acer Aspire A715-79G. It uses the Clevo/ACPI fan
interface and ships a kernel driver plus a Rust daemon.

## What it does

- Loads `acer_fan` at boot through DKMS
- Exposes fan temperatures and speeds in sysfs
- Runs `acer-fand` to apply a temperature based fan curve
- Accepts commands through `fan-mode` and `fan-speed`
- Supports profiles: `silent`, `quiet`, `balanced`, `cool`, `freezy`, `custom`

## Requirements

- Linux with systemd
- Kernel headers, compiler, and DKMS
- Rust toolchain

## Quick start

```sh
sudo ./scripts/install-system.sh
sudo ./scripts/validate-driver.sh
fan-mode auto
```

## Commands

```sh
fan-mode auto
fan-mode firmware-auto
fan-mode manual 40
fan-mode off
fan-mode profile balanced
fan-speed 40
```

`fan-mode auto` uses the daemon curve. `fan-mode firmware-auto` lets
the firmware control the fans.

## Files

- `driver/` - `acer_fan` kernel module
- `src/` - `acer-fand` daemon
- `scripts/` - install, uninstall, validation, and control scripts
- `packaging/` - systemd unit and module autoload files
- `docs/architecture.md` - design notes

## Uninstall

```sh
sudo ./scripts/uninstall.sh
```
