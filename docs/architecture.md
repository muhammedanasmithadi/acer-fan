# acer-fan architecture

`acer_fan.ko` binds the ACPI device `CLV0001:00`. It exposes fan
temperature, speed, and mode through sysfs.

`acer-fand` is a Rust daemon with no external crates. It:

- reads `/etc/acer-fan.conf`
- reads fan temperatures every second
- applies a filtered fan curve
- writes fan speeds through sysfs
- accepts commands through `/run/acer-fan/cmd`

## Command protocol

Write one command to `/run/acer-fan/cmd`:

| Command | Effect |
|---|---|
| `set auto` | Use the daemon curve |
| `set firmware-auto` | Use firmware automatic mode |
| `set manual <0-100>` | Use a fixed speed |
| `set off` | Turn fans off |
| `profile <name>` | Select a fan profile |
| `speed <0-100>` | Set the manual speed |
| `stop` | Restore firmware auto and exit |

`fan-mode` and `fan-speed` send these commands for you.

## Profiles

Profiles come from TUXEDO Control Center defaults:

- `silent`
- `quiet`
- `balanced`
- `cool`
- `freezy`
- `custom`

## Fan curve

Each fan has its own temperature buffer. The daemon filters recent
readings, looks up a speed in the active profile, clamps it to the
configured limits, and applies hardware limits. It also limits how fast
the speed can fall and raises the minimum speed at high temperatures.
