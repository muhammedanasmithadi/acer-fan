# acer_fan - self-owned fan control driver

Single-module kernel driver for the Clevo/ACPI fan interface exposed by
the Acer Aspire A715-79G. It is firmware-mediated through ACPI `_DSM`;
there is no direct port I/O, MMIO, or EC RAM access.

## Sysfs interface

Under `/sys/devices/platform/acer_fan/`:

- `fan1_temp`, `fan2_temp`, `fan3_temp`: Celsius
- `fan1_speed`, `fan2_speed`, `fan3_speed`: percent, 0-100
- `fan_mode`: `auto` or `manual`
- `fan_min_speed`: 20
- `fan_max_speed`: 100
- `fans_off_available`: 1
- `fan_count`: 3

Writing `fan1_speed` or `fan2_speed` sets manual mode. Writing `fan_mode`
accepts `auto` or `manual`; `auto` asks firmware for automatic control.
