# acer_fan driver

Single kernel module for Clevo/ACPI fan control on the Acer Aspire
A715-79G. It uses ACPI `_DSM`. It does not use direct port I/O, MMIO,
or EC RAM.

## Sysfs interface

Under `/sys/devices/platform/acer_fan/`:

- `fan1_temp`, `fan2_temp`, `fan3_temp`: Celsius
- `fan1_speed`, `fan2_speed`, `fan3_speed`: percent, 0 to 100
- `fan_mode`: `auto` or `manual`
- `fan_min_speed`: 20
- `fan_max_speed`: 100
- `fans_off_available`: 1
- `fan_count`: 3

Writing a fan speed sets manual mode. Writing `fan_mode` accepts
`auto` or `manual`. `auto` asks the firmware for automatic control.
