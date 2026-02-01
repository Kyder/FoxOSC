#![no_std]

use core::panic::PanicInfo;

// Host functions
extern "C" {
    fn get_system_time() -> u32;
    fn osc_send_float(addr_ptr: *const u8, addr_len: u32, value: f32) -> i32;
    fn log_info(msg_ptr: *const u8, msg_len: u32);
    fn log_error(msg_ptr: *const u8, msg_len: u32);
    fn save_config(key_ptr: *const u8, key_len: u32, value_ptr: *const u8, value_len: u32);
    fn load_config(key_ptr: *const u8, key_len: u32) -> i32; // Returns ptr to value or 0
}

// Plugin state
static mut RUNNING: bool = false;
static mut LAST_SECOND: u32 = 255;
static mut LAST_MINUTE: u32 = 255;
static mut LAST_HOUR: u32 = 255;
static mut TICK_COUNT: u32 = 0;
static mut LAST_MINUTE_SEND: u32 = 0;
static mut LAST_HOUR_SEND: u32 = 0;

// Configuration storage
static mut CONFIG_SECONDS: [u8; 128] = [0; 128];
static mut CONFIG_MINUTES: [u8; 128] = [0; 128];
static mut CONFIG_HOURS: [u8; 128] = [0; 128];
static mut CONFIG_LENS: (usize, usize, usize) = (0, 0, 0);

// Default addresses
static SECONDS_ADDR: &str = "/avatar/parameters/Time_Seconds";
static MINUTES_ADDR: &str = "/avatar/parameters/Time_Minutes";
static HOURS_ADDR: &str = "/avatar/parameters/Time_Hours";

// Convert frame index to the exact 2-decimal float Unity expects
// Unity truncates to 2 decimals then does floor(value * total_frames)
// So we need: ceil(frame * 100 / total_frames) / 100
fn frame_to_value(frame: u32, total_frames: u32) -> f32 {
    if frame == 0 {
        return 0.0;
    }
    // Integer ceil: (a + b - 1) / b
    let numerator = frame * 100 + total_frames - 1;
    let cents = numerator / total_frames; // This is ceil(frame*100/total_frames)
    cents as f32 / 100.0
}

fn send_float(address: &str, value: f32) {
    unsafe {
        osc_send_float(address.as_ptr(), address.len() as u32, value);
    }
}

fn log(message: &str) {
    unsafe {
        log_info(message.as_ptr(), message.len() as u32);
    }
}

fn save_config_value(key: &str, value: &str) {
    unsafe {
        save_config(
            key.as_ptr(), key.len() as u32,
            value.as_ptr(), value.len() as u32
        );
    }
}

fn load_config_value(key: &str) -> Option<&'static str> {
    unsafe {
        let ptr = load_config(key.as_ptr(), key.len() as u32);
        if ptr == 0 {
            return None;
        }
        
        // Read from fixed memory location (ptr points to length + data)
        let len_bytes = core::slice::from_raw_parts(ptr as *const u8, 4);
        let len = u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
        
        let value_bytes = core::slice::from_raw_parts((ptr + 4) as *const u8, len);
        Some(core::str::from_utf8_unchecked(value_bytes))
    }
}

fn get_current_time() -> (u32, u32, u32) {
    unsafe {
        let packed = get_system_time();
        let hour = (packed >> 16) & 0xFF;
        let minute = (packed >> 8) & 0xFF;
        let second = packed & 0xFF;
        (second, minute, hour)
    }
}

fn get_seconds_addr() -> &'static str {
    unsafe {
        if CONFIG_LENS.0 > 0 {
            core::str::from_utf8_unchecked(&CONFIG_SECONDS[..CONFIG_LENS.0])
        } else {
            SECONDS_ADDR
        }
    }
}

fn get_minutes_addr() -> &'static str {
    unsafe {
        if CONFIG_LENS.1 > 0 {
            core::str::from_utf8_unchecked(&CONFIG_MINUTES[..CONFIG_LENS.1])
        } else {
            MINUTES_ADDR
        }
    }
}

fn get_hours_addr() -> &'static str {
    unsafe {
        if CONFIG_LENS.2 > 0 {
            core::str::from_utf8_unchecked(&CONFIG_HOURS[..CONFIG_LENS.2])
        } else {
            HOURS_ADDR
        }
    }
}

#[no_mangle]
pub extern "C" fn plugin_info() -> *const u8 {
    let json = r#"{"name":"Watch","version":"0.1.0","description":"Sends current time (seconds, minutes, hours) to VRChat"}"#;
    write_string(json)
}

#[no_mangle]
pub extern "C" fn plugin_ui_config() -> *const u8 {
    let json = r#"{"title":"Watch","elements":[{"Label":{"text":"Configure OSC addresses for time values"}},{"Separator":null},{"TextInput":{"id":"seconds","label":"Seconds:","default_value":"/avatar/parameters/Time_Seconds","placeholder":"OSC address"}},{"TextInput":{"id":"minutes","label":"Minutes:","default_value":"/avatar/parameters/Time_Minutes","placeholder":"OSC address"}},{"TextInput":{"id":"hours","label":"Hours:","default_value":"/avatar/parameters/Time_Hours","placeholder":"OSC address"}}]}"#;
    write_string(json)
}

#[no_mangle]
pub extern "C" fn plugin_load_config() {
    // Load addresses from config
    if let Some(addr) = load_config_value("seconds_address") {
        unsafe {
            let len = addr.len().min(127);
            CONFIG_SECONDS[..len].copy_from_slice(&addr.as_bytes()[..len]);
            CONFIG_LENS.0 = len;
        }
    }
    
    if let Some(addr) = load_config_value("minutes_address") {
        unsafe {
            let len = addr.len().min(127);
            CONFIG_MINUTES[..len].copy_from_slice(&addr.as_bytes()[..len]);
            CONFIG_LENS.1 = len;
        }
    }
    
    if let Some(addr) = load_config_value("hours_address") {
        unsafe {
            let len = addr.len().min(127);
            CONFIG_HOURS[..len].copy_from_slice(&addr.as_bytes()[..len]);
            CONFIG_LENS.2 = len;
        }
    }
}

#[no_mangle]
pub extern "C" fn plugin_ui_event(event_ptr: i32, event_len: i32) {
    unsafe {
        let event_bytes = core::slice::from_raw_parts(event_ptr as *const u8, event_len as usize);
        let event_str = core::str::from_utf8_unchecked(event_bytes);
        
        if event_str.contains("ApplySettings") {
            if let Some(seconds_start) = event_str.find(r#""seconds",""#) {
                if let Some(seconds_end) = event_str[seconds_start + 11..].find('"') {
                    let addr = &event_str[seconds_start + 11..seconds_start + 11 + seconds_end];
                    let len = addr.len().min(127);
                    CONFIG_SECONDS[..len].copy_from_slice(&addr.as_bytes()[..len]);
                    CONFIG_LENS.0 = len;
                    save_config_value("seconds_address", addr);
                }
            }
            
            if let Some(minutes_start) = event_str.find(r#""minutes",""#) {
                if let Some(minutes_end) = event_str[minutes_start + 11..].find('"') {
                    let addr = &event_str[minutes_start + 11..minutes_start + 11 + minutes_end];
                    let len = addr.len().min(127);
                    CONFIG_MINUTES[..len].copy_from_slice(&addr.as_bytes()[..len]);
                    CONFIG_LENS.1 = len;
                    save_config_value("minutes_address", addr);
                }
            }
            
            if let Some(hours_start) = event_str.find(r#""hours",""#) {
                if let Some(hours_end) = event_str[hours_start + 9..].find('"') {
                    let addr = &event_str[hours_start + 9..hours_start + 9 + hours_end];
                    let len = addr.len().min(127);
                    CONFIG_HOURS[..len].copy_from_slice(&addr.as_bytes()[..len]);
                    CONFIG_LENS.2 = len;
                    save_config_value("hours_address", addr);
                }
            }
            
            log("Configuration saved");
        }
    }
}

#[no_mangle]
pub extern "C" fn plugin_start() {
    unsafe {
        RUNNING = true;
        LAST_SECOND = 255;
        LAST_MINUTE = 255;
        LAST_HOUR = 255;
        TICK_COUNT = 0;
        LAST_MINUTE_SEND = 0;
        LAST_HOUR_SEND = 0;
    }
    log("Watch plugin started");
}

#[no_mangle]
pub extern "C" fn plugin_stop() {
    unsafe {
        RUNNING = false;
    }
    log("Watch plugin stopped");
}

#[no_mangle]
pub extern "C" fn plugin_update() {
    unsafe {
        if !RUNNING {
            return;
        }
        
        TICK_COUNT += 1;
        
        let (second, minute, hour) = get_current_time();
        
        // Send seconds every second (every time it changes)
        if second != LAST_SECOND {
            let seconds_norm = frame_to_value(second, 60);
            send_float(get_seconds_addr(), seconds_norm);
            LAST_SECOND = second;
        }
        
        // Send minutes: immediately when value changes OR every 50 ticks (5 seconds)
        let minute_changed = minute != LAST_MINUTE;
        let minute_interval_elapsed = TICK_COUNT - LAST_MINUTE_SEND >= 50;
        
        if minute_changed || minute_interval_elapsed {
            let minutes_norm = frame_to_value(minute, 60);
            send_float(get_minutes_addr(), minutes_norm);
            LAST_MINUTE = minute;
            LAST_MINUTE_SEND = TICK_COUNT;
        }
        
        // Send hours: immediately when value changes OR every 50 ticks (5 seconds)
        let hour_changed = hour != LAST_HOUR;
        let hour_interval_elapsed = TICK_COUNT - LAST_HOUR_SEND >= 50;
        
        if hour_changed || hour_interval_elapsed {
            let hours_norm = frame_to_value(hour, 24);
            send_float(get_hours_addr(), hours_norm);
            LAST_HOUR = hour;
            LAST_HOUR_SEND = TICK_COUNT;
        }
    }
}

fn write_string(s: &str) -> *const u8 {
    let bytes = s.as_bytes();
    let len = bytes.len() as u32;
    
    unsafe {
        let ptr = alloc(4 + len as usize);
        *(ptr as *mut u32) = len;
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(4), bytes.len());
        ptr
    }
}

static mut HEAP: [u8; 65536] = [0; 65536];
static mut HEAP_POS: usize = 0;

unsafe fn alloc(size: usize) -> *mut u8 {
    let ptr = HEAP.as_mut_ptr().add(HEAP_POS);
    HEAP_POS += size;
    ptr
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}