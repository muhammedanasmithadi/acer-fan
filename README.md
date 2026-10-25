# acer-fan

Fan control for the Acer Aspire A715-79G using its Clevo/ACPI fan
interface, with a self-owned kernel driver and a small Rust daemon.

## What it does

- Loads `acer_fan` at boot
- Exposes fan temperatures and speeds through sysfs
- Runs `acer-fand`, which applies a temperature-based fan curve
- Provides `fan-mode` and `fan-speed` commands
- Supports profiles: `silent`, `quiet`, `balanced`, `cool`, `freezy`, `custom`

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

## Files

- `driver/` - `acer_fan` kernel module
- `src/` - `acer-fand` daemon
- `scripts/` - install, uninstall, validation, control scripts
- `packaging/` - systemd unit and module autoload
- `docs/architecture.md` - design notes
