#![no_std]

use core::panic::PanicInfo;

// Host functions
extern "C" {
    fn get_system_time() -> u32;
    fn get_unix_timestamp() -> u64;
    fn osc_send_float(addr_ptr: *const u8, addr_len: u32, value: f32) -> i32;
    fn osc_send_chatbox(msg_ptr: *const u8, msg_len: u32, typing: i32) -> i32;
    fn log_info(msg_ptr: *const u8, msg_len: u32);
    fn log_error(msg_ptr: *const u8, msg_len: u32);
    fn save_config(key_ptr: *const u8, key_len: u32, value_ptr: *const u8, value_len: u32);
    fn load_config(key_ptr: *const u8, key_len: u32) -> i32;
}

// Plugin state
static mut RUNNING: bool = false;
static mut LAST_BOOP_STATE: bool = false;
static mut TODAY_BOOPS: u32 = 0;
static mut TOTAL_BOOPS: u32 = 0;
static mut LAST_CHATBOX_SEND: u32 = 0;
static mut TICK_COUNT: u32 = 0;
static mut PENDING_BOOP: bool = false;

// Last boop timestamp (Unix timestamp - seconds since epoch)
static mut LAST_BOOP_TIMESTAMP: u64 = 0;

// Configuration storage
static mut BOOP_INPUT_ADDR: [u8; 128] = [0; 128];
static mut BOOP_INPUT_LEN: usize = 0;

// Default address WITH LEADING SLASH
static DEFAULT_BOOP_INPUT: &str = "/avatar/parameters/OSCBoop";

// UI command flags
static mut SEND_MSG_FLAG: bool = false;
static mut RESET_TODAY_FLAG: bool = false;

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
        
        let len_bytes = core::slice::from_raw_parts(ptr as *const u8, 4);
        let len = u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
        
        let value_bytes = core::slice::from_raw_parts((ptr + 4) as *const u8, len);
        Some(core::str::from_utf8_unchecked(value_bytes))
    }
}

// Check if two Unix timestamps are on different days
fn is_different_day(ts1: u64, ts2: u64) -> bool {
    // One day = 86400 seconds
    let day1 = ts1 / 86400;
    let day2 = ts2 / 86400;
    day1 != day2
}

fn get_boop_input_addr() -> &'static str {
    unsafe {
        if BOOP_INPUT_LEN > 0 {
            core::str::from_utf8_unchecked(&BOOP_INPUT_ADDR[..BOOP_INPUT_LEN])
        } else {
            DEFAULT_BOOP_INPUT
        }
    }
}

fn u32_to_str(num: u32, buffer: &mut [u8]) -> &str {
    if num == 0 {
        buffer[0] = b'0';
        return unsafe { core::str::from_utf8_unchecked(&buffer[..1]) };
    }
    
    let mut n = num;
    let mut i = 0;
    let mut temp = [0u8; 10];
    
    while n > 0 {
        temp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    
    // Reverse
    for j in 0..i {
        buffer[j] = temp[i - 1 - j];
    }
    
    unsafe { core::str::from_utf8_unchecked(&buffer[..i]) }
}

fn u64_to_str(num: u64, buffer: &mut [u8]) -> &str {
    if num == 0 {
        buffer[0] = b'0';
        return unsafe { core::str::from_utf8_unchecked(&buffer[..1]) };
    }
    
    let mut n = num;
    let mut i = 0;
    let mut temp = [0u8; 20];
    
    while n > 0 {
        temp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    
    // Reverse
    for j in 0..i {
        buffer[j] = temp[i - 1 - j];
    }
    
    unsafe { core::str::from_utf8_unchecked(&buffer[..i]) }
}

fn send_chatbox_message() {
    unsafe {
        // Build message: "Today: X\nTotal Boops: Y"
        let mut msg_buffer = [0u8; 256];
        let mut pos = 0;
        
        // "Today: "
        let prefix = b"Today: ";
        msg_buffer[pos..pos + prefix.len()].copy_from_slice(prefix);
        pos += prefix.len();
        
        // Number
        let mut num_buf = [0u8; 10];
        let today_str = u32_to_str(TODAY_BOOPS, &mut num_buf);
        msg_buffer[pos..pos + today_str.len()].copy_from_slice(today_str.as_bytes());
        pos += today_str.len();
        
        // "\nTotal Boops: "
        let middle = b"\nTotal Boops: ";
        msg_buffer[pos..pos + middle.len()].copy_from_slice(middle);
        pos += middle.len();
        
        // Total number
        let mut num_buf2 = [0u8; 10];
        let total_str = u32_to_str(TOTAL_BOOPS, &mut num_buf2);
        msg_buffer[pos..pos + total_str.len()].copy_from_slice(total_str.as_bytes());
        pos += total_str.len();
        
        let message = core::str::from_utf8_unchecked(&msg_buffer[..pos]);
        
        // typing=1 (true) = send immediately
        osc_send_chatbox(message.as_ptr(), message.len() as u32, 1);
        
        log("Chatbox message sent");
    }
}

fn save_counters() {
    unsafe {
        let mut buf = [0u8; 10];
        let today_str = u32_to_str(TODAY_BOOPS, &mut buf);
        save_config_value("today_boops", today_str);
        
        let mut buf2 = [0u8; 10];
        let total_str = u32_to_str(TOTAL_BOOPS, &mut buf2);
        save_config_value("total_boops", total_str);
        
        // Save timestamp
        let mut buf3 = [0u8; 20];
        let ts_str = u64_to_str(LAST_BOOP_TIMESTAMP, &mut buf3);
        save_config_value("last_boop_timestamp", ts_str);
    }
}

fn str_to_u32(s: &str) -> u32 {
    let mut result = 0u32;
    for b in s.as_bytes() {
        if *b >= b'0' && *b <= b'9' {
            result = result * 10 + (*b - b'0') as u32;
        }
    }
    result
}

fn str_to_u64(s: &str) -> u64 {
    let mut result = 0u64;
    for b in s.as_bytes() {
        if *b >= b'0' && *b <= b'9' {
            result = result * 10 + (*b - b'0') as u64;
        }
    }
    result
}

#[no_mangle]
pub extern "C" fn plugin_info() -> *const u8 {
    let json = r#"{"name":"Boop Counter","version":"0.1.0","description":"Counts boops and sends chatbox messages"}"#;
    write_string(json)
}

#[no_mangle]
pub extern "C" fn plugin_ui_config() -> *const u8 {
    // Build UI WITHOUT static counter labels (those are added by the core app)
    unsafe {
        let current_addr = get_boop_input_addr();
        
        // Simple UI: just the config and buttons, NO counter labels
        let json = r#"{"title":"Boop Counter","elements":[{"Label":{"text":"OSC Configuration"}},{"TextInput":{"id":"boop_input","label":"Boop Input:","default_value":""#;
        
        let mut buffer = [0u8; 512];
        let mut pos = 0;
        
        buffer[pos..pos + json.len()].copy_from_slice(json.as_bytes());
        pos += json.len();
        
        // Add current address
        buffer[pos..pos + current_addr.len()].copy_from_slice(current_addr.as_bytes());
        pos += current_addr.len();
        
        // Buttons
        let end = br#"","placeholder":"OSC address"}},{"Separator":null},{"Button":{"id":"send_msg","label":"Send Boop Message"}},{"Button":{"id":"reset_today","label":"Reset Today Boops (undo test boops)"}}]}"#;
        buffer[pos..pos + end.len()].copy_from_slice(end);
        pos += end.len();
        
        let json_str = core::str::from_utf8_unchecked(&buffer[..pos]);
        write_string(json_str)
    }
}

#[no_mangle]
pub extern "C" fn plugin_load_config() {
    // Load address
    if let Some(addr) = load_config_value("boop_input_address") {
        unsafe {
            let len = addr.len().min(127);
            BOOP_INPUT_ADDR[..len].copy_from_slice(&addr.as_bytes()[..len]);
            BOOP_INPUT_LEN = len;
        }
        log("Loaded boop address from config");
    } else {
        // Initialize with default
        unsafe {
            let default_addr = DEFAULT_BOOP_INPUT;
            let len = default_addr.len();
            BOOP_INPUT_ADDR[..len].copy_from_slice(default_addr.as_bytes());
            BOOP_INPUT_LEN = len;
        }
        log("Using default boop address");
    }
    
    // Load counters
    if let Some(today) = load_config_value("today_boops") {
        unsafe {
            TODAY_BOOPS = str_to_u32(today);
        }
    }
    
    if let Some(total) = load_config_value("total_boops") {
        unsafe {
            TOTAL_BOOPS = str_to_u32(total);
        }
    }
    
    // Load last boop timestamp
    if let Some(ts) = load_config_value("last_boop_timestamp") {
        unsafe {
            LAST_BOOP_TIMESTAMP = str_to_u64(ts);
        }
    }
    
    // Check if we need to reset today's boops
    unsafe {
        let current_ts = get_unix_timestamp();
        
        if LAST_BOOP_TIMESTAMP > 0 && is_different_day(LAST_BOOP_TIMESTAMP, current_ts) {
            TODAY_BOOPS = 0;
            save_counters();
            log("New day detected on startup - today boops reset to 0");
        }
        
        // Update timestamp to current
        LAST_BOOP_TIMESTAMP = current_ts;
    }
    
    // Log what we loaded
    unsafe {
        let mut msg = [0u8; 256];
        let mut pos = 0;
        
        let prefix = b"Loaded: Today=";
        msg[pos..pos + prefix.len()].copy_from_slice(prefix);
        pos += prefix.len();
        
        let mut buf = [0u8; 10];
        let today_str = u32_to_str(TODAY_BOOPS, &mut buf);
        msg[pos..pos + today_str.len()].copy_from_slice(today_str.as_bytes());
        pos += today_str.len();
        
        let mid = b" Total=";
        msg[pos..pos + mid.len()].copy_from_slice(mid);
        pos += mid.len();
        
        let mut buf2 = [0u8; 10];
        let total_str = u32_to_str(TOTAL_BOOPS, &mut buf2);
        msg[pos..pos + total_str.len()].copy_from_slice(total_str.as_bytes());
        pos += total_str.len();
        
        let full_msg = core::str::from_utf8_unchecked(&msg[..pos]);
        log(full_msg);
    }
    
    // Log listening address
    unsafe {
        let addr = get_boop_input_addr();
        let mut msg = [0u8; 256];
        let prefix = b"Listening to OSC: ";
        msg[..prefix.len()].copy_from_slice(prefix);
        msg[prefix.len()..prefix.len() + addr.len()].copy_from_slice(addr.as_bytes());
        let full_msg = core::str::from_utf8_unchecked(&msg[..prefix.len() + addr.len()]);
        log(full_msg);
    }
}

#[no_mangle]
pub extern "C" fn plugin_ui_event(event_ptr: i32, event_len: i32) {
    unsafe {
        let event_bytes = core::slice::from_raw_parts(event_ptr as *const u8, event_len as usize);
        let event_str = core::str::from_utf8_unchecked(event_bytes);
        
        // Check for button clicks
        if event_str.contains(r#""ButtonClicked""#) {
            if event_str.contains(r#""send_msg""#) {
                SEND_MSG_FLAG = true;
            } else if event_str.contains(r#""reset_today""#) {
                RESET_TODAY_FLAG = true;
            }
        }
        
        // Check for ApplySettings
        if event_str.contains("ApplySettings") {
            // Look for: ["boop_input","VALUE"]
            if let Some(start) = event_str.find(r#"["boop_input",""#) {
                let search_start = start + 16;
                if let Some(end) = event_str[search_start..].find(r#""]"#) {
                    let addr = &event_str[search_start..search_start + end];
                    let len = addr.len().min(127);
                    
                    // Save to memory
                    BOOP_INPUT_ADDR[..len].copy_from_slice(&addr.as_bytes()[..len]);
                    BOOP_INPUT_LEN = len;
                    
                    // Save to config file
                    save_config_value("boop_input_address", addr);
                    
                    // Log it
                    let mut msg = [0u8; 256];
                    let prefix = b"Saved new address: ";
                    msg[..prefix.len()].copy_from_slice(prefix);
                    msg[prefix.len()..prefix.len() + addr.len()].copy_from_slice(addr.as_bytes());
                    let full_msg = core::str::from_utf8_unchecked(&msg[..prefix.len() + addr.len()]);
                    log(full_msg);
                }
            }
        }
    }
}

// This will be called by the host when OSC message arrives
#[no_mangle]
pub extern "C" fn plugin_on_osc_bool(value: i32) {
    unsafe {
        let is_true = value != 0;
        
        // Detect rising edge (false -> true)
        if is_true && !LAST_BOOP_STATE {
            PENDING_BOOP = true;
            log("BOOP DETECTED!");
        }
        
        LAST_BOOP_STATE = is_true;
    }
}

#[no_mangle]
pub extern "C" fn plugin_start() {
    unsafe {
        RUNNING = true;
        LAST_BOOP_STATE = false;
        TICK_COUNT = 0;
        LAST_CHATBOX_SEND = 0;
        PENDING_BOOP = false;
    }
    log("Boop Counter plugin started");
}

#[no_mangle]
pub extern "C" fn plugin_stop() {
    unsafe {
        RUNNING = false;
    }
    save_counters();
    log("Boop Counter plugin stopped");
}

#[no_mangle]
pub extern "C" fn plugin_update() {
    unsafe {
        if !RUNNING {
            return;
        }
        
        TICK_COUNT += 1;
        
        // Check for day change every ~10 minutes (6000 ticks at 100ms)
        if TICK_COUNT % 6000 == 0 {
            let current_ts = get_unix_timestamp();
            if LAST_BOOP_TIMESTAMP > 0 && is_different_day(LAST_BOOP_TIMESTAMP, current_ts) {
                TODAY_BOOPS = 0;
                LAST_BOOP_TIMESTAMP = current_ts;
                save_counters();
                log("Day changed during runtime - today boops reset");
            }
        }
        
        // Handle pending boop
        if PENDING_BOOP {
            PENDING_BOOP = false;
            
            // Increment counters
            TODAY_BOOPS += 1;
            TOTAL_BOOPS += 1;
            
            // Update timestamp to NOW
            LAST_BOOP_TIMESTAMP = get_unix_timestamp();
            
            save_counters();
            
            log("Boop counted!");
            
            // Send chatbox message with 2-second cooldown (20 ticks at 100ms)
            if TICK_COUNT - LAST_CHATBOX_SEND >= 20 {
                send_chatbox_message();
                LAST_CHATBOX_SEND = TICK_COUNT;
            } else {
                log("Chatbox on cooldown");
            }
        }
        
        // Handle UI button presses
        if SEND_MSG_FLAG {
            SEND_MSG_FLAG = false;
            send_chatbox_message();
            LAST_CHATBOX_SEND = TICK_COUNT;
        }
        
        if RESET_TODAY_FLAG {
            RESET_TODAY_FLAG = false;
            
            // Subtract today's boops from total ONLY if today > 0
            if TODAY_BOOPS > 0 {
                // Prevent underflow
                if TOTAL_BOOPS >= TODAY_BOOPS {
                    TOTAL_BOOPS -= TODAY_BOOPS;
                } else {
                    TOTAL_BOOPS = 0;
                }
                
                TODAY_BOOPS = 0;
                save_counters();
                
                log("Today boops reset - removed from total");
            } else {
                log("Today already at 0 - no reset needed");
            }
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