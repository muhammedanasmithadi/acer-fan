/* SPDX-License-Identifier: GPL-2.0-or-later */
/*
 * acer_fan.h - Clevo/ACPI fan control for the Acer Aspire A715-79G
 *
 * Derived from tuxedo-drivers (Copyright (c) 2020-2026 TUXEDO Computers
 * GmbH), reduced to the Clevo fan-control path used by this machine.
 */
#ifndef ACER_FAN_H
#define ACER_FAN_H

#include <linux/types.h>

#define ACER_FAN_ACPI_HID "CLV0001"
#define ACER_FAN_DSM_UUID "93f224e4-fbdc-4bbf-add6-db71bdc0afad"

#define ACER_FAN_CMD_GET_FANINFO1 0x63
#define ACER_FAN_CMD_GET_FANINFO2 0x64
#define ACER_FAN_CMD_GET_FANINFO3 0x6e
#define ACER_FAN_CMD_SET_FANSPEED 0x68
#define ACER_FAN_CMD_SET_FANS_AUTO 0x69
#define ACER_FAN_CMD_GET_BIOS_FEATURES_1 0x52

#endif
