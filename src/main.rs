mod profiles;

use std::fmt;
use std::fs;
use std::io::Read;
use std::os::raw::c_int;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

extern "C" {
    fn flock(fd: c_int, operation: c_int) -> c_int;
    fn signal(signum: c_int, handler: extern "C" fn(c_int)) -> extern "C" fn(c_int);
}

const LOCK_EX: c_int = 2;
const LOCK_UN: c_int = 8;
const SIGTERM: c_int = 15;
const SIGINT: c_int = 2;

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn handle_signal(_sig: c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}

const SYSFS: &str = "/sys/devices/platform/acer_fan";
const CONFIG: &str = "/etc/acer-fan.conf";
const CMD: &str = "/run/acer-fan/cmd";
const DEFAULT_PROFILE: &str = "balanced";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Auto,
    FirmwareAuto,
    Manual,
    Off,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::FirmwareAuto => "firmware-auto",
            Self::Manual => "manual",
            Self::Off => "off",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "firmware-auto" => Some(Self::FirmwareAuto),
            "manual" => Some(Self::Manual),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone)]
struct Config {
    mode: Mode,
    profile: String,
    min_speed: i32,
    max_speed: i32,
    offset: i32,
    manual_speed: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Auto,
            profile: DEFAULT_PROFILE.to_string(),
            min_speed: 20,
            max_speed: 100,
            offset: 0,
            manual_speed: 40,
        }
    }
}

struct ValueBuffer {
    data: Vec<i32>,
}

impl ValueBuffer {
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn add(&mut self, value: i32) {
        self.data.push(value);
        while self.data.len() > 13 {
            self.data.remove(0);
        }
    }

    fn filtered(&self) -> Option<i32> {
        if self.data.is_empty() {
            return None;
        }

        let mut sorted = self.data.clone();
        sorted.sort();

        let n = sorted.len();
        let middle_index = n.div_ceil(2);
        let half_size = (7 + 1) / 2;
        let start = (middle_index as isize - half_size as isize).max(0) as usize;
        let half_size_offset = if n % 2 == 1 { 3 } else { 4 };
        let end = (middle_index + half_size_offset).min(n);

        if start >= end {
            return None;
        }

        let slice = &sorted[start..end];
        let sum: i32 = slice.iter().sum();
        Some((sum as f64 / slice.len() as f64).round() as i32)
    }
}

struct FanLogic {
    table: &'static [u8; 101],
    buffer: ValueBuffer,
    last_speed: i32,
    latest: i32,
    min_speed: i32,
    max_speed: i32,
    offset: i32,
}

impl FanLogic {
    fn new(table: &'static [u8; 101], min_speed: i32, max_speed: i32, offset: i32) -> Self {
        Self {
            table,
            buffer: ValueBuffer::new(),
            last_speed: 0,
            latest: -1,
            min_speed,
            max_speed,
            offset,
        }
    }

    fn set_profile(
        &mut self,
        table: &'static [u8; 101],
        min_speed: i32,
        max_speed: i32,
        offset: i32,
    ) {
        self.table = table;
        self.min_speed = min_speed;
        self.max_speed = max_speed;
        self.offset = offset;
    }

    fn report_temperature(&mut self, temperature: i32) {
        self.buffer.add(temperature);
        let next = self.calculate();
        if next > -1 {
            self.latest = next;
        }
    }

    fn calculate(&mut self) -> i32 {
        let temp = match self.buffer.filtered() {
            Some(t) => t,
            None => return -1,
        };

        let speed = if temp < 0 {
            self.table[0] as i32
        } else if temp as usize >= self.table.len() {
            *self.table.last().unwrap() as i32
        } else {
            self.table[temp as usize] as i32
        };

        let mut speed = speed + self.offset;
        speed = speed.clamp(self.min_speed, self.max_speed).clamp(0, 100);
        speed = self.apply_hw_fan_limitations(speed);
        let jump = speed - self.last_speed;
        if self.last_speed > 20 && jump <= -2 {
            speed = self.last_speed - 2;
        }
        speed = self.manage_critical_temperature(temp, speed);
        self.last_speed = speed;
        speed
    }

    fn apply_hw_fan_limitations(&self, speed: i32) -> i32 {
        const MIN_SPEED: i32 = 20;
        const HALF_MIN_SPEED: i32 = 10;
        const FANS_OFF_AVAILABLE: bool = true;

        if speed < MIN_SPEED {
            if FANS_OFF_AVAILABLE && speed < HALF_MIN_SPEED {
                0
            } else if FANS_OFF_AVAILABLE || speed >= HALF_MIN_SPEED {
                MIN_SPEED
            } else {
                speed
            }
        } else {
            speed
        }
    }

    fn manage_critical_temperature(&self, temp: i32, speed: i32) -> i32 {
        if temp >= 90 {
            speed.max(40)
        } else if temp >= 80 {
            speed.max(30)
        } else {
            speed
        }
    }

    fn speed(&self) -> i32 {
        self.latest
    }
}

struct FileLock {
    fd: c_int,
}

impl FileLock {
    fn exclusive(fd: c_int) -> Result<Self, std::io::Error> {
        loop {
            let rc = unsafe { flock(fd, LOCK_EX) };
            if rc == 0 {
                return Ok(Self { fd });
            }
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::Interrupted {
                return Err(err);
            }
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        unsafe {
            flock(self.fd, LOCK_UN);
        }
    }
}

struct DaemonState {
    config: Config,
    profile_name: String,
    cpu: FanLogic,
    gpu: FanLogic,
    gpu2: FanLogic,
    last_written: Option<i32>,
    firmware_auto_applied: bool,
}

impl DaemonState {
    fn new() -> Self {
        let config = load_config();
        let profile_name = config.profile.clone();
        let profile =
            profiles::profile(&profile_name).unwrap_or(profiles::profile(DEFAULT_PROFILE).unwrap());
        let cpu = FanLogic::new(
            &profile.cpu,
            config.min_speed,
            config.max_speed,
            config.offset,
        );
        let gpu = FanLogic::new(
            &profile.gpu,
            config.min_speed,
            config.max_speed,
            config.offset,
        );
        let gpu2 = FanLogic::new(
            &profile.gpu,
            config.min_speed,
            config.max_speed,
            config.offset,
        );

        Self {
            config,
            profile_name,
            cpu,
            gpu,
            gpu2,
            last_written: None,
            firmware_auto_applied: false,
        }
    }

    fn apply_profile(&mut self, name: &str) {
        if let Some(table) = profiles::profile(name) {
            self.profile_name = name.to_string();
            self.config.profile = name.to_string();
            self.cpu.set_profile(
                &table.cpu,
                self.config.min_speed,
                self.config.max_speed,
                self.config.offset,
            );
            self.gpu.set_profile(
                &table.gpu,
                self.config.min_speed,
                self.config.max_speed,
                self.config.offset,
            );
            self.gpu2.set_profile(
                &table.gpu,
                self.config.min_speed,
                self.config.max_speed,
                self.config.offset,
            );
            self.last_written = None;
            persist_config(&self.config);
        } else {
            eprintln!("acer-fand: unknown profile '{name}'");
        }
    }

    fn handle_command(&mut self, cmd: &str) -> bool {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        match parts.as_slice() {
            ["auto"] | ["set", "auto"] => {
                self.config.mode = Mode::Auto;
                let _ = write_sysfs(sysfs_path("/fan_mode"), "manual");
                self.last_written = None;
                persist_config(&self.config);
            }
            ["firmware-auto"] | ["set", "firmware-auto"] => {
                self.config.mode = Mode::FirmwareAuto;
                let _ = write_sysfs(sysfs_path("/fan_mode"), "auto");
                self.last_written = None;
                persist_config(&self.config);
            }
            ["manual", speed] | ["set", "manual", speed] => {
                if let Ok(n) = speed.parse::<i32>() {
                    if (0..=100).contains(&n) {
                        self.config.manual_speed = n;
                        self.config.mode = Mode::Manual;
                        let _ = write_sysfs(sysfs_path("/fan_mode"), "manual");
                        self.last_written = None;
                        persist_config(&self.config);
                    }
                }
            }
            ["off"] | ["set", "off"] => {
                self.config.mode = Mode::Off;
                let _ = write_sysfs(sysfs_path("/fan_mode"), "manual");
                self.last_written = None;
                persist_config(&self.config);
            }
            ["profile", name] => {
                self.apply_profile(name);
            }
            ["speed", n] => {
                if let Ok(n) = n.parse::<i32>() {
                    if (0..=100).contains(&n) {
                        self.config.manual_speed = n;
                        self.config.mode = Mode::Manual;
                        let _ = write_sysfs(sysfs_path("/fan_mode"), "manual");
                        self.last_written = None;
                        persist_config(&self.config);
                    }
                }
            }
            ["stop"] => {
                let _ = write_sysfs(sysfs_path("/fan_mode"), "auto");
                return true;
            }
            _ => eprintln!("acer-fand: invalid command '{cmd}'"),
        }
        false
    }
}

fn load_config() -> Config {
    let mut config = Config::default();
    if let Ok(text) = fs::read_to_string(CONFIG) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                match key.trim() {
                    "mode" => {
                        if let Some(mode) = Mode::parse(value.trim()) {
                            config.mode = mode;
                        }
                    }
                    "profile" => {
                        if profiles::profile(value.trim()).is_some() {
                            config.profile = value.trim().to_string();
                        }
                    }
                    "min_speed" => {
                        if let Ok(v) = value.trim().parse::<i32>() {
                            config.min_speed = v.clamp(0, 100);
                        }
                    }
                    "max_speed" => {
                        if let Ok(v) = value.trim().parse::<i32>() {
                            config.max_speed = v.clamp(0, 100);
                        }
                    }
                    "offset" => {
                        if let Ok(v) = value.trim().parse::<i32>() {
                            config.offset = v.clamp(-100, 100);
                        }
                    }
                    "manual_speed" => {
                        if let Ok(v) = value.trim().parse::<i32>() {
                            config.manual_speed = v.clamp(0, 100);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if config.min_speed > config.max_speed {
        config.min_speed = config.max_speed;
    }
    config
}

fn persist_config(config: &Config) {
    let text = format!(
        "mode={}\nprofile={}\nmin_speed={}\nmax_speed={}\noffset={}\nmanual_speed={}\n",
        config.mode,
        config.profile,
        config.min_speed,
        config.max_speed,
        config.offset,
        config.manual_speed
    );
    let temporary = format!("{CONFIG}.tmp");
    if fs::write(&temporary, text).is_ok() {
        if let Err(err) = fs::rename(&temporary, CONFIG) {
            eprintln!("acer-fand: failed to replace config: {err}");
            let _ = fs::remove_file(&temporary);
        }
    } else {
        eprintln!("acer-fand: failed to write config");
    }
}

fn sysfs_path(name: &str) -> String {
    format!("{SYSFS}/{name}")
}

fn read_i32<P: AsRef<str>>(path: P) -> Result<i32, std::io::Error> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)?;
    text.trim()
        .parse::<i32>()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid integer"))
}

fn write_sysfs<P: AsRef<str>>(path: P, value: &str) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    fs::write(path, format!("{value}\n"))
}

fn read_command() -> Option<String> {
    let mut f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(CMD)
        .ok()?;

    let fd = f.as_raw_fd();
    let _lock = FileLock::exclusive(fd).ok()?;
    let mut content = String::new();
    f.read_to_string(&mut content).ok()?;
    let _ = f.set_len(0);
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn main() {
    if let Err(err) = fs::create_dir_all("/run/acer-fan") {
        eprintln!("acer-fand: failed to create runtime directory: {err}");
        return;
    }

    if let Ok(f) = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(CMD)
    {
        let _ = f.set_permissions(fs::Permissions::from_mode(0o666));
    } else {
        eprintln!("acer-fand: failed to create command file");
    }

    let mut state = DaemonState::new();

    unsafe {
        signal(SIGTERM, handle_signal);
        signal(SIGINT, handle_signal);
    }

    if state.config.mode == Mode::FirmwareAuto {
        if write_sysfs(sysfs_path("/fan_mode"), "auto").is_ok() {
            state.firmware_auto_applied = true;
        }
    } else {
        let _ = write_sysfs(sysfs_path("/fan_mode"), "manual");
    }

    eprintln!("acer-fand starting (pid {})", process::id());

    loop {
        if !RUNNING.load(Ordering::SeqCst) {
            break;
        }

        if let Some(cmd) = read_command() {
            if state.handle_command(&cmd) {
                break;
            }
            if state.config.mode == Mode::FirmwareAuto
                && !state.firmware_auto_applied
                && write_sysfs(sysfs_path("/fan_mode"), "auto").is_ok()
            {
                state.firmware_auto_applied = true;
            }
        }

        let mut sleep_for = Duration::from_secs(1);

        match state.config.mode {
            Mode::Auto => {
                let t0 = read_i32(sysfs_path("/fan1_temp"));
                let t1 = read_i32(sysfs_path("/fan2_temp"));
                let t2 = read_i32(sysfs_path("/fan3_temp"));
                if let (Ok(t0), Ok(t1), Ok(t2)) = (t0, t1, t2) {
                    state.cpu.report_temperature(t0);
                    state.gpu.report_temperature(t1);
                    state.gpu2.report_temperature(t2);
                    let s0 = state.cpu.speed();
                    let s1 = state.gpu.speed();
                    let s2 = state.gpu2.speed();
                    if s0 >= 0 && s1 >= 0 && s2 >= 0 {
                        let max_speed = s0.max(s1).max(s2);
                        if state.last_written != Some(max_speed) {
                            let ok1 =
                                write_sysfs(sysfs_path("/fan1_speed"), &max_speed.to_string())
                                    .is_ok();
                            let ok2 =
                                write_sysfs(sysfs_path("/fan2_speed"), &max_speed.to_string())
                                    .is_ok();
                            let ok3 =
                                write_sysfs(sysfs_path("/fan3_speed"), &max_speed.to_string())
                                    .is_ok();
                            if ok1 && ok2 && ok3 {
                                state.last_written = Some(max_speed);
                            }
                        }
                    }
                } else {
                    sleep_for = Duration::from_secs(5);
                }
            }
            Mode::Manual => {
                if state.last_written != Some(state.config.manual_speed) {
                    let ok1 = write_sysfs(
                        sysfs_path("/fan1_speed"),
                        &state.config.manual_speed.to_string(),
                    )
                    .is_ok();
                    let ok2 = write_sysfs(
                        sysfs_path("/fan2_speed"),
                        &state.config.manual_speed.to_string(),
                    )
                    .is_ok();
                    let ok3 = write_sysfs(
                        sysfs_path("/fan3_speed"),
                        &state.config.manual_speed.to_string(),
                    )
                    .is_ok();
                    if ok1 && ok2 && ok3 {
                        state.last_written = Some(state.config.manual_speed);
                    }
                }
            }
            Mode::Off => {
                if state.last_written != Some(0) {
                    let ok1 = write_sysfs(sysfs_path("/fan1_speed"), "0").is_ok();
                    let ok2 = write_sysfs(sysfs_path("/fan2_speed"), "0").is_ok();
                    let ok3 = write_sysfs(sysfs_path("/fan3_speed"), "0").is_ok();
                    if ok1 && ok2 && ok3 {
                        state.last_written = Some(0);
                    }
                }
            }
            Mode::FirmwareAuto => {
                if !state.firmware_auto_applied
                    && write_sysfs(sysfs_path("/fan_mode"), "auto").is_ok()
                {
                    state.firmware_auto_applied = true;
                }
            }
        }

        sleep(sleep_for);
    }

    let _ = write_sysfs(sysfs_path("/fan_mode"), "auto");
    eprintln!("acer-fand stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_lookup() {
        assert!(profiles::profile("balanced").is_some());
        assert!(profiles::profile("nope").is_none());
    }

    #[test]
    fn value_buffer_filters() {
        let mut b = ValueBuffer::new();
        for v in [10, 20, 30] {
            b.add(v);
        }
        assert_eq!(b.filtered(), Some(20));
    }

    #[test]
    fn fan_logic_reports_constant_profile_speed() {
        const TABLE: [u8; 101] = [50u8; 101];
        let mut logic = FanLogic::new(&TABLE, 20, 100, 0);
        for _ in 0..13 {
            logic.report_temperature(60);
        }
        assert_eq!(logic.speed(), 50);
    }

    #[test]
    fn mode_round_trips() {
        for (text, mode) in [
            ("auto", Mode::Auto),
            ("firmware-auto", Mode::FirmwareAuto),
            ("manual", Mode::Manual),
            ("off", Mode::Off),
        ] {
            assert_eq!(Mode::parse(text), Some(mode));
            assert_eq!(mode.as_str(), text);
        }
    }
}
