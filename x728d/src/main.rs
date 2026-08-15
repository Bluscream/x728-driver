use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use rppal::gpio::{Gpio, Level, Trigger};
use rppal::i2c::I2c;
use serde_json::Value;

const MAX17048_ADDR: u16 = 0x36;
const REG_VCELL: u8 = 0x02;
const REG_SOC: u8 = 0x04;

const PIN_PLD: u8 = 6;       // AC Power Loss Detect (GPIO 6)
const PIN_BUTTON: u8 = 5;     // Verified X728 Power Button (GPIO 5)
const PIN_BUZZER: u8 = 20;    // X728 Buzzer Control (GPIO 20)

const SHUTDOWN_GRACE_PERIOD_SECS: u64 = 120;
const SHM_STATE_PATH: &str = "/dev/shm/x728_state.json";
const SHM_CMD_PATH: &str = "/dev/shm/x728_cmd.json";

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let readonly = args.iter().any(|arg| arg == "-ro" || arg == "--readonly");

    let start_time = Instant::now();

    println!("==================================================");
    println!(" Geekworm X728 Native Event-Driven Daemon (v7.2)");
    println!(" Mode: {}", if readonly { "READONLY (Diagnostics Only)" } else { "ACTIVE (Production)" });
    println!(" Architecture: Hardware Interrupts / JSON IPC Commands");
    println!(" Shared Memory State: {}", SHM_STATE_PATH);
    println!(" Shared Memory Command: {}", SHM_CMD_PATH);
    println!(" Safety Grace Period: {} seconds", SHUTDOWN_GRACE_PERIOD_SECS);
    println!("==================================================");

    let mut i2c_option = I2c::new().ok();
    if let Some(ref mut i2c) = i2c_option {
        let _ = i2c.set_slave_address(MAX17048_ADDR);
    }

    let gpio = match Gpio::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("[x728d] Failed to initialize GPIO: {}", e);
            return;
        }
    };

    let mut buzzer_output = match gpio.get(PIN_BUZZER) {
        Ok(pin) => pin.into_output_low(),
        Err(e) => {
            eprintln!("[x728d] Error initializing GPIO 20 (Buzzer): {}", e);
            return;
        }
    };
    buzzer_output.set_low();

    let mut pld_input = match gpio.get(PIN_PLD) {
        Ok(pin) => pin.into_input_pullup(),
        Err(e) => {
            eprintln!("[x728d] Error reading GPIO 6 (PLD): {}", e);
            return;
        }
    };

    let mut button_input = match gpio.get(PIN_BUTTON) {
        Ok(pin) => pin.into_input_pulldown(),
        Err(e) => {
            eprintln!("[x728d] Error reading GPIO 5 (Button): {}", e);
            return;
        }
    };

    let button_pressed = Arc::new(AtomicBool::new(button_input.is_high()));
    let button_last_pressed_ms = Arc::new(AtomicU64::new(if button_input.is_high() { current_timestamp_ms() } else { 0 }));
    let ac_unplugged = Arc::new(AtomicBool::new(pld_input.is_high()));

    let b_flag = button_pressed.clone();
    let b_ts = button_last_pressed_ms.clone();
    let _ = button_input.set_async_interrupt(Trigger::Both, move |level| {
        let pressed = level == Level::High;
        b_flag.store(pressed, Ordering::SeqCst);
        if pressed {
            b_ts.store(current_timestamp_ms(), Ordering::SeqCst);
        }
    });

    let p_flag = ac_unplugged.clone();
    let _ = pld_input.set_async_interrupt(Trigger::Both, move |level| {
        let unplugged = level == Level::High;
        p_flag.store(unplugged, Ordering::SeqCst);
    });

    println!("[x728d] Hardware interrupts attached. Ready for IPC command JSON files at {}.", SHM_CMD_PATH);

    let mut button_press_duration_secs = 0;
    let mut manual_buzzer_active = false;

    loop {
        let elapsed = start_time.elapsed().as_secs();
        let in_grace_period = elapsed < SHUTDOWN_GRACE_PERIOD_SECS;

        let cur_button = button_pressed.load(Ordering::SeqCst);
        let last_pressed_ms = button_last_pressed_ms.load(Ordering::SeqCst);
        let cur_unplugged = ac_unplugged.load(Ordering::SeqCst);

        // Check for IPC command file /dev/shm/x728_cmd.json
        if fs::metadata(SHM_CMD_PATH).is_ok() {
            if let Ok(content) = fs::read_to_string(SHM_CMD_PATH) {
                let _ = fs::remove_file(SHM_CMD_PATH);
                if let Ok(parsed) = serde_json::from_str::<Value>(&content) {
                    println!("[x728d] Received JSON command: {}", content);
                    if let Some(b) = parsed.get("buzzer").and_then(|v| v.as_bool()) {
                        manual_buzzer_active = b;
                        println!("[x728d] Manual buzzer state set to: {}", b);
                    }
                    if parsed.get("reboot").and_then(|v| v.as_bool()).unwrap_or(false) {
                        if !readonly && !in_grace_period {
                            println!("[x728d] Executing system reboot...");
                            let _ = Command::new("reboot").status();
                        }
                    }
                    if parsed.get("shutdown").and_then(|v| v.as_bool()).unwrap_or(false) {
                        if !readonly && !in_grace_period {
                            println!("[x728d] Executing system shutdown...");
                            let _ = Command::new("shutdown").args(&["-h", "now"]).status();
                        }
                    }
                }
            }
        }

        // Apply Buzzer Output: Manual IPC state or Power Button override
        if manual_buzzer_active {
            buzzer_output.set_high();
        } else {
            buzzer_output.set_low();
        }

        let mut voltage = 4.10f32;
        let mut capacity = 95.0f32;

        if let Some(ref mut i2c) = i2c_option {
            let mut buf_vcell = [0u8; 2];
            let mut buf_soc = [0u8; 2];
            if i2c.write_read(&[REG_VCELL], &mut buf_vcell).is_ok() && i2c.write_read(&[REG_SOC], &mut buf_soc).is_ok() {
                let vcell_raw = u16::from_be_bytes(buf_vcell);
                let soc_raw = u16::from_be_bytes(buf_soc);
                voltage = (vcell_raw as f32) * 1.25 / 1000.0 / 16.0;
                capacity = (soc_raw as f32) / 256.0;
            }
        }

        let json_payload = format!(
            "{{\"voltage\": {:.2}, \"capacity\": {:.1}, \"ac_power\": {}, \"button_pressed\": {}, \"button_last_pressed_ms\": {}, \"buzzer_active\": {}, \"readonly\": {}}}",
            voltage,
            capacity,
            if !cur_unplugged { "true" } else { "false" },
            if cur_button { "true" } else { "false" },
            last_pressed_ms,
            if manual_buzzer_active { "true" } else { "false" },
            if readonly { "true" } else { "false" }
        );

        if let Ok(mut shm_file) = File::create(SHM_STATE_PATH) {
            let _ = shm_file.write_all(json_payload.as_bytes());
        }

        // Physical Power Button Multi-tier Durations & Buzzer Muting
        if cur_button {
            button_press_duration_secs += 1;
            if button_press_duration_secs >= 10 {
                manual_buzzer_active = false;
                buzzer_output.set_low();
            }
        } else {
            if button_press_duration_secs > 0 && button_press_duration_secs < 1 {
                manual_buzzer_active = false;
                buzzer_output.set_low();
            } else if button_press_duration_secs >= 1 && button_press_duration_secs <= 3 {
                if !readonly && !in_grace_period {
                    let _ = Command::new("reboot").status();
                }
            } else if button_press_duration_secs > 3 && button_press_duration_secs < 10 {
                if !readonly && !in_grace_period {
                    let _ = Command::new("shutdown").args(&["-h", "now"]).status();
                }
            } else if button_press_duration_secs >= 10 {
                manual_buzzer_active = false;
                buzzer_output.set_low();
            }
            button_press_duration_secs = 0;
        }

        thread::sleep(Duration::from_secs(1));
    }
}
