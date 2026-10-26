/* SPDX-License-Identifier: GPL-2.0-or-later */
#define pr_fmt(fmt) KBUILD_MODNAME ": " fmt

#include <linux/acpi.h>
#include <linux/module.h>
#include <linux/platform_device.h>
#include <linux/sysfs.h>
#include <linux/mutex.h>
#include <linux/slab.h>
#include <linux/mod_devicetable.h>
#include <linux/uuid.h>
#include <linux/delay.h>

#include "acer_fan.h"

static struct acpi_device *acer_fan_adev;
static struct platform_device *acer_fan_pdev;
static DEFINE_MUTEX(acer_fan_lock);
static int acer_fan_mode; /* 0 = auto, 1 = manual */
static int acer_fan_speeds[3] = { -1, -1, -1 };
static int acer_fan_speeds_valid;

static int acer_fan_eval(u8 cmd, u32 arg, u32 *result)
{
    guid_t uuid;
    acpi_handle handle;
    union acpi_object argv4_data[1];
    union acpi_object argv4;
    union acpi_object *out_obj;
    int status;

    if (!acer_fan_adev)
        return -ENODEV;

    status = guid_parse(ACER_FAN_DSM_UUID, &uuid);
    if (status < 0)
        return -ENOENT;

    handle = acpi_device_handle(acer_fan_adev);
    if (!handle)
        return -ENODEV;

    argv4_data[0].type = ACPI_TYPE_INTEGER;
    argv4_data[0].integer.value = arg;
    argv4.type = ACPI_TYPE_PACKAGE;
    argv4.package.count = 1;
    argv4.package.elements = argv4_data;

    out_obj = acpi_evaluate_dsm(handle, &uuid, 0, cmd, &argv4);
    if (!out_obj) {
        pr_err("failed to evaluate _DSM command 0x%02x\n", cmd);
        return -EIO;
    }

    if (result) {
        if (out_obj->type == ACPI_TYPE_INTEGER)
            *result = (u32)out_obj->integer.value;
        else
            *result = 0;
    }

    ACPI_FREE(out_obj);
    return 0;
}

static int acer_fan_get_info(int fan, u32 *value)
{
    u8 cmd;

    switch (fan) {
    case 0:
        cmd = ACER_FAN_CMD_GET_FANINFO1;
        break;
    case 1:
        cmd = ACER_FAN_CMD_GET_FANINFO2;
        break;
    default:
        cmd = ACER_FAN_CMD_GET_FANINFO3;
        break;
    }

    return acer_fan_eval(cmd, 0, value);
}

static int acer_fan_get_temp(int fan)
{
    u32 info;
    int err;

    err = acer_fan_get_info(fan, &info);
    if (err)
        return err;

    return (int)((info >> 16) & 0xff);
}

static int acer_fan_get_speed_raw(int fan)
{
    u32 info;
    int err;

    err = acer_fan_get_info(fan, &info);
    if (err)
        return err;

    return (int)(info & 0xff);
}

static int acer_fan_get_speed_percent(int fan)
{
    int raw = acer_fan_get_speed_raw(fan);

    if (raw < 0)
        return raw;

    return (raw * 100 + 127) / 255;
}

static int acer_fan_set_speed_percent(int fan, int percent)
{
    u32 info;
    u32 arg;
    int raw;
    int i;
    int err;

    if (fan < 0 || fan > 2)
        return -EINVAL;
    if (percent < 0 || percent > 100)
        return -EINVAL;

    /* Refresh cached speeds when coming from firmware auto mode. */
    if (!acer_fan_speeds_valid || acer_fan_mode == 0) {
        for (i = 0; i < 3; i++) {
            err = acer_fan_get_info(i, &info);
            if (err)
                return err;
            acer_fan_speeds[i] = (int)(info & 0xff);
        }
        acer_fan_speeds_valid = 1;
    }

    raw = (percent * 255 + 50) / 100;
    acer_fan_speeds[fan] = raw;
    arg = (u32)acer_fan_speeds[0] |
          ((u32)acer_fan_speeds[1] << 8) |
          ((u32)acer_fan_speeds[2] << 16);

    err = acer_fan_eval(ACER_FAN_CMD_SET_FANSPEED, arg, NULL);
    if (err)
        return err;

    /* The Clevo firmware needs time to apply the new fan speeds. */
    msleep(100);

    acer_fan_mode = 1;
    return 0;
}

static int acer_fan_set_auto(void)
{
    int err;

    err = acer_fan_eval(ACER_FAN_CMD_SET_FANS_AUTO, 15, NULL);
    if (err)
        return err;

    acer_fan_speeds_valid = 0;
    acer_fan_mode = 0;
    return 0;
}

static ssize_t fan1_temp_show(struct device *dev, struct device_attribute *attr, char *buf)
{
    int temp;

    mutex_lock(&acer_fan_lock);
    temp = acer_fan_get_temp(0);
    mutex_unlock(&acer_fan_lock);

    if (temp < 0)
        return temp;

    return sysfs_emit(buf, "%d\n", temp);
}

static ssize_t fan2_temp_show(struct device *dev, struct device_attribute *attr, char *buf)
{
    int temp;

    mutex_lock(&acer_fan_lock);
    temp = acer_fan_get_temp(1);
    mutex_unlock(&acer_fan_lock);

    if (temp < 0)
        return temp;

    return sysfs_emit(buf, "%d\n", temp);
}

static ssize_t fan1_speed_show(struct device *dev, struct device_attribute *attr, char *buf)
{
    int speed;

    mutex_lock(&acer_fan_lock);
    speed = acer_fan_get_speed_percent(0);
    mutex_unlock(&acer_fan_lock);

    if (speed < 0)
        return speed;

    return sysfs_emit(buf, "%d\n", speed);
}

static ssize_t fan2_speed_show(struct device *dev, struct device_attribute *attr, char *buf)
{
    int speed;

    mutex_lock(&acer_fan_lock);
    speed = acer_fan_get_speed_percent(1);
    mutex_unlock(&acer_fan_lock);

    if (speed < 0)
        return speed;

    return sysfs_emit(buf, "%d\n", speed);
}

static ssize_t fan1_speed_store(struct device *dev, struct device_attribute *attr,
                               const char *buf, size_t count)
{
    int speed;
    int err;

    if (kstrtoint(buf, 10, &speed))
        return -EINVAL;
    if (speed < 0 || speed > 100)
        return -EINVAL;

    mutex_lock(&acer_fan_lock);
    err = acer_fan_set_speed_percent(0, speed);
    mutex_unlock(&acer_fan_lock);

    if (err)
        return err;

    return count;
}

static ssize_t fan2_speed_store(struct device *dev, struct device_attribute *attr,
                               const char *buf, size_t count)
{
    int speed;
    int err;

    if (kstrtoint(buf, 10, &speed))
        return -EINVAL;
    if (speed < 0 || speed > 100)
        return -EINVAL;

    mutex_lock(&acer_fan_lock);
    err = acer_fan_set_speed_percent(1, speed);
    mutex_unlock(&acer_fan_lock);

    if (err)
        return err;

    return count;
}

static ssize_t fan3_temp_show(struct device *dev, struct device_attribute *attr, char *buf)
{
    int temp;

    mutex_lock(&acer_fan_lock);
    temp = acer_fan_get_temp(2);
    mutex_unlock(&acer_fan_lock);

    if (temp < 0)
        return temp;

    return sysfs_emit(buf, "%d\n", temp);
}

static ssize_t fan3_speed_show(struct device *dev, struct device_attribute *attr, char *buf)
{
    int speed;

    mutex_lock(&acer_fan_lock);
    speed = acer_fan_get_speed_percent(2);
    mutex_unlock(&acer_fan_lock);

    if (speed < 0)
        return speed;

    return sysfs_emit(buf, "%d\n", speed);
}

static ssize_t fan3_speed_store(struct device *dev, struct device_attribute *attr,
                               const char *buf, size_t count)
{
    int speed;
    int err;

    if (kstrtoint(buf, 10, &speed))
        return -EINVAL;
    if (speed < 0 || speed > 100)
        return -EINVAL;

    mutex_lock(&acer_fan_lock);
    err = acer_fan_set_speed_percent(2, speed);
    mutex_unlock(&acer_fan_lock);

    if (err)
        return err;

    return count;
}

static ssize_t fan_mode_show(struct device *dev, struct device_attribute *attr, char *buf)
{
    return sysfs_emit(buf, "%s\n", acer_fan_mode ? "manual" : "auto");
}

static ssize_t fan_mode_store(struct device *dev, struct device_attribute *attr,
                             const char *buf, size_t count)
{
    int err;

    mutex_lock(&acer_fan_lock);
    if (sysfs_streq(buf, "auto")) {
        err = acer_fan_set_auto();
    } else if (sysfs_streq(buf, "manual")) {
        acer_fan_mode = 1;
        acer_fan_speeds_valid = 0;
        err = 0;
    } else {
        err = -EINVAL;
    }
    mutex_unlock(&acer_fan_lock);

    if (err)
        return err;

    return count;
}

static ssize_t fan_min_speed_show(struct device *dev, struct device_attribute *attr, char *buf)
{
    return sysfs_emit(buf, "20\n");
}

static ssize_t fan_max_speed_show(struct device *dev, struct device_attribute *attr, char *buf)
{
    return sysfs_emit(buf, "100\n");
}

static ssize_t fans_off_available_show(struct device *dev, struct device_attribute *attr, char *buf)
{
    return sysfs_emit(buf, "1\n");
}

static ssize_t fan_count_show(struct device *dev, struct device_attribute *attr, char *buf)
{
    return sysfs_emit(buf, "3\n");
}

static DEVICE_ATTR_RO(fan1_temp);
static DEVICE_ATTR_RW(fan1_speed);
static DEVICE_ATTR_RO(fan2_temp);
static DEVICE_ATTR_RW(fan2_speed);
static DEVICE_ATTR_RO(fan3_temp);
static DEVICE_ATTR_RW(fan3_speed);
static DEVICE_ATTR_RW(fan_mode);
static DEVICE_ATTR_RO(fan_min_speed);
static DEVICE_ATTR_RO(fan_max_speed);
static DEVICE_ATTR_RO(fans_off_available);
static DEVICE_ATTR_RO(fan_count);

static struct attribute *acer_fan_attrs[] = {
    &dev_attr_fan1_temp.attr,
    &dev_attr_fan1_speed.attr,
    &dev_attr_fan2_temp.attr,
    &dev_attr_fan2_speed.attr,
    &dev_attr_fan3_temp.attr,
    &dev_attr_fan3_speed.attr,
    &dev_attr_fan_mode.attr,
    &dev_attr_fan_min_speed.attr,
    &dev_attr_fan_max_speed.attr,
    &dev_attr_fans_off_available.attr,
    &dev_attr_fan_count.attr,
    NULL,
};

static const struct attribute_group acer_fan_attr_group = {
    .attrs = acer_fan_attrs,
};

static int acer_fan_probe(struct platform_device *pdev)
{
    struct acpi_device *adev = ACPI_COMPANION(&pdev->dev);
    u32 result;
    int err;

    if (!adev)
        return -ENODEV;

    acer_fan_adev = adev;
    err = acer_fan_eval(ACER_FAN_CMD_GET_BIOS_FEATURES_1, 0, &result);
    /* The Clevo method uses 0x52 as a general probe. */
    if (err) {
        pr_err("probe method 0x52 failed: %d\n", err);
        return err;
    }
    if (result == 0xffffffff) {
        pr_err("probe method 0x52 returned 0xffffffff\n");
        return -ENODEV;
    }
    pr_info("probe method 0x52 returned 0x%08x\n", result);

    acer_fan_adev = adev;

    acer_fan_pdev = platform_device_register_simple("acer_fan", PLATFORM_DEVID_NONE, NULL, 0);
    if (IS_ERR(acer_fan_pdev)) {
        acer_fan_adev = NULL;
        return PTR_ERR(acer_fan_pdev);
    }

    err = sysfs_create_group(&acer_fan_pdev->dev.kobj, &acer_fan_attr_group);
    if (err) {
        platform_device_unregister(acer_fan_pdev);
        acer_fan_pdev = NULL;
        acer_fan_adev = NULL;
        return err;
    }

    dev_info(&pdev->dev, "interface initialized\n");
    return 0;
}

static void acer_fan_remove(struct platform_device *pdev)
{
    if (acer_fan_pdev) {
        sysfs_remove_group(&acer_fan_pdev->dev.kobj, &acer_fan_attr_group);
        platform_device_unregister(acer_fan_pdev);
        acer_fan_pdev = NULL;
    }
    acer_fan_adev = NULL;
}

static const struct acpi_device_id acer_fan_acpi_ids[] = {
    { ACER_FAN_ACPI_HID, 0 },
    { },
};

static struct platform_driver acer_fan_platform_driver = {
    .probe = acer_fan_probe,
    .remove = acer_fan_remove,
    .driver = {
        .name = "acer_fan",
        .acpi_match_table = acer_fan_acpi_ids,
        .owner = THIS_MODULE,
    },
};

module_platform_driver(acer_fan_platform_driver);

MODULE_AUTHOR("Derived from tuxedo-drivers");
MODULE_DESCRIPTION("Clevo/ACPI fan control for Acer Aspire A715-79G");
MODULE_LICENSE("GPL v2");
MODULE_VERSION("1.0.0");
MODULE_DEVICE_TABLE(acpi, acer_fan_acpi_ids);
