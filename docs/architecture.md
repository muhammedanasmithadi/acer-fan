# acer-fan architecture

`acer_fan.ko` binds the ACPI device `CLV0001:00` and exposes fan
temperature, speed, and mode through sysfs. `acer-fand` is a
dependency-free Rust daemon that:

- reads `/etc/acer-fan.conf`
- reads fan temperatures from sysfs every second
- applies a temperature-filtered fan curve
- writes fan speeds through sysfs
- accepts commands through `/run/acer-fan/cmd`

Commands:

- `set auto` - daemon curve
- `set firmware-auto` - firmware automatic mode
- `set manual <0-100>` - fixed speed
- `set off` - fans off
- `profile <name>` - select a profile
- `speed <0-100>` - fixed speed
- `stop` - restore firmware auto and exit

Profiles are taken from TUXEDO Control Center defaults: `silent`,
`quiet`, `balanced`, `cool`, `freezy`, and `custom`.
