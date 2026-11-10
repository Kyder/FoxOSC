#![no_std]

use core::panic::PanicInfo;

// Host functions, imported from the "env" module the app provides.
// Newer Rust no longer turns undefined externs into wasm imports
// automatically, so the import module must be named explicitly.
#[link(wasm_import_module = "env")]
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

// Custom messages storage - array of (number, message) pairs
// Max 10 custom messages, each message max 20 chars
static mut CUSTOM_MESSAGES: [(u32, [u8; 20], usize); 10] = [(0, [0; 20], 0); 10];
static mut CUSTOM_MESSAGE_COUNT: usize = 0;

// Default address WITH LEADING SLASH
static DEFAULT_BOOP_INPUT: &str = "/avatar/parameters/OSCBoop";

// UI command flags
static mut SEND_MSG_FLAG: bool = false;
static mut RESET_TODAY_FLAG: bool = false;

// Quiet mode: boops are still counted and saved, but nothing is posted to the
// chatbox automatically. Saved as config key "quiet_mode".
static mut QUIET_MODE: bool = false;

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

// Local timezone offset in seconds, derived from the host's local clock
// (get_system_time) vs UTC (get_unix_timestamp). No extra host function needed.
fn local_utc_offset() -> i64 {
    unsafe {
        let packed = get_system_time();
        let local_sod = (((packed >> 16) & 0xFF) * 3600 + ((packed >> 8) & 0xFF) * 60 + (packed & 0xFF)) as i64;
        let utc_sod = (get_unix_timestamp() % 86400) as i64;
        let mut off = local_sod - utc_sod;
        // Wrap into the real-world range (-12h .. +14h)
        if off > 14 * 3600 {
            off -= 86400;
        } else if off < -12 * 3600 {
            off += 86400;
        }
        off
    }
}

// Check if two Unix timestamps are on different *local* days.
// (Was plain UTC days, so "today" reset at 01:00/02:00 in Prague, not midnight.)
fn is_different_day(ts1: u64, ts2: u64) -> bool {
    let off = local_utc_offset();
    let day1 = (ts1 as i64 + off).div_euclid(86400);
    let day2 = (ts2 as i64 + off).div_euclid(86400);
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

// Format u32 with space thousand separators
// Examples: 256 -> "256", 1024 -> "1 024", 122588 -> "122 588"
fn u32_to_str_with_spaces(num: u32, buffer: &mut [u8]) -> &str {
    if num == 0 {
        buffer[0] = b'0';
        return unsafe { core::str::from_utf8_unchecked(&buffer[..1]) };
    }
    
    // First convert to plain digits
    let mut n = num;
    let mut i = 0;
    let mut temp = [0u8; 10];
    
    while n > 0 {
        temp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    
    // Now write to buffer with spaces, reversed
    let digit_count = i;
    let mut pos = 0;
    
    for j in 0..digit_count {
        // Add space before every group of 3, but not at the start
        if j > 0 && j % 3 == 0 {
            buffer[pos] = b' ';
            pos += 1;
        }
        buffer[pos] = temp[digit_count - 1 - j];
        pos += 1;
    }
    
    unsafe { core::str::from_utf8_unchecked(&buffer[..pos]) }
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
        // Build message: "Today: X [message]\nTotal Boops: Y"
        let mut msg_buffer = [0u8; 256];
        let mut pos = 0;
        
        // "Today: "
        let prefix = b"Today: ";
        msg_buffer[pos..pos + prefix.len()].copy_from_slice(prefix);
        pos += prefix.len();
        
        // Number with spaces
        let mut num_buf = [0u8; 15]; // Larger buffer for spaces
        let today_str = u32_to_str_with_spaces(TODAY_BOOPS, &mut num_buf);
        msg_buffer[pos..pos + today_str.len()].copy_from_slice(today_str.as_bytes());
        pos += today_str.len();
        
        // Add custom message if matched
        let custom_msg = get_custom_message_for_today();
        if !custom_msg.is_empty() {
            msg_buffer[pos] = b' ';
            pos += 1;
            msg_buffer[pos..pos + custom_msg.len()].copy_from_slice(custom_msg.as_bytes());
            pos += custom_msg.len();
        }
        
        // "\nTotal Boops: "
        let middle = b"\nTotal Boops: ";
        msg_buffer[pos..pos + middle.len()].copy_from_slice(middle);
        pos += middle.len();
        
        // Total number with spaces
        let mut num_buf2 = [0u8; 15]; // Larger buffer for spaces
        let total_str = u32_to_str_with_spaces(TOTAL_BOOPS, &mut num_buf2);
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

fn save_custom_messages() {
    unsafe {
        // Save the count first so load knows how many entries are valid
        let mut count_buf = [0u8; 10];
        let count_str = u32_to_str(CUSTOM_MESSAGE_COUNT as u32, &mut count_buf);
        save_config_value("msg_count", count_str);

        for i in 0..CUSTOM_MESSAGE_COUNT {
            let (number, msg_bytes, msg_len) = &CUSTOM_MESSAGES[i];
            
            // Build key: msg0, msg1, etc
            let mut key_buf = [0u8; 20];
            let mut key_pos = 0;
            let prefix = b"msg";
            key_buf[key_pos..key_pos + prefix.len()].copy_from_slice(prefix);
            key_pos += prefix.len();
            
            let mut idx_buf = [0u8; 10];
            let idx_str = u32_to_str(i as u32, &mut idx_buf);
            key_buf[key_pos..key_pos + idx_str.len()].copy_from_slice(idx_str.as_bytes());
            key_pos += idx_str.len();
            
            let key = core::str::from_utf8_unchecked(&key_buf[..key_pos]);
            
            // Build value: "UwU,69"
            let mut value_buf = [0u8; 40];
            let mut value_pos = 0;
            
            let msg_str = core::str::from_utf8_unchecked(&msg_bytes[..*msg_len]);
            value_buf[value_pos..value_pos + msg_str.len()].copy_from_slice(msg_str.as_bytes());
            value_pos += msg_str.len();
            
            value_buf[value_pos] = b',';
            value_pos += 1;
            
            let mut num_buf = [0u8; 10];
            let num_str = u32_to_str(*number, &mut num_buf);
            value_buf[value_pos..value_pos + num_str.len()].copy_from_slice(num_str.as_bytes());
            value_pos += num_str.len();
            
            let value = core::str::from_utf8_unchecked(&value_buf[..value_pos]);
            save_config_value(key, value);
        }
    }
}

fn load_custom_messages() {
    unsafe {
        CUSTOM_MESSAGE_COUNT = 0;

        // Read how many messages were saved (prevents stale entries from reappearing)
        let saved_count = if let Some(count_str) = load_config_value("msg_count") {
            str_to_u32(count_str) as usize
        } else {
            10 // Legacy: no count key, try all 10 slots
        };

        let limit = if saved_count < 10 { saved_count } else { 10 };

        // Try to load up to `limit` messages: msg0, msg1, etc
        for i in 0..limit {
            // Build key: msg0, msg1, etc
            let mut key_buf = [0u8; 20];
            let mut key_pos = 0;
            let prefix = b"msg";
            key_buf[key_pos..key_pos + prefix.len()].copy_from_slice(prefix);
            key_pos += prefix.len();
            
            let mut idx_buf = [0u8; 10];
            let idx_str = u32_to_str(i as u32, &mut idx_buf);
            key_buf[key_pos..key_pos + idx_str.len()].copy_from_slice(idx_str.as_bytes());
            key_pos += idx_str.len();
            
            let key = core::str::from_utf8_unchecked(&key_buf[..key_pos]);
            
            if let Some(value) = load_config_value(key) {
                if value.is_empty() {
                    continue; // Skip empty entries
                }
                
                // Parse "UwU,69" format
                if let Some(comma_pos) = value.find(',') {
                    let msg_text = &value[..comma_pos];
                    let num_text = &value[comma_pos + 1..];
                    
                    if msg_text.is_empty() {
                        continue;
                    }
                    
                    let number = str_to_u32(num_text);
                    if number == 0 {
                        continue; // Skip if number is 0 or invalid
                    }
                    
                    let msg_len = msg_text.len().min(20);
                    let mut msg_bytes = [0u8; 20];
                    msg_bytes[..msg_len].copy_from_slice(&msg_text.as_bytes()[..msg_len]);
                    
                    CUSTOM_MESSAGES[CUSTOM_MESSAGE_COUNT] = (number, msg_bytes, msg_len);
                    CUSTOM_MESSAGE_COUNT += 1;
                }
            }
        }
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

// Check if today's boop count contains any of the custom message numbers
// Returns the message if found, otherwise empty string
fn get_custom_message_for_today() -> &'static str {
    unsafe {
        if TODAY_BOOPS == 0 || CUSTOM_MESSAGE_COUNT == 0 {
            return "";
        }
        
        let today_str_buf = &mut [0u8; 10];
        let today_str = u32_to_str(TODAY_BOOPS, today_str_buf);
        
        // Check each custom message
        for i in 0..CUSTOM_MESSAGE_COUNT {
            let (number, msg_bytes, msg_len) = &CUSTOM_MESSAGES[i];
            
            if *number == 0 {
                continue;
            }
            
            // Convert number to string
            let mut num_buf = [0u8; 10];
            let num_str = u32_to_str(*number, &mut num_buf);
            
            // Check if today's count contains this number
            if today_str.contains(num_str) {
                return core::str::from_utf8_unchecked(&msg_bytes[..*msg_len]);
            }
        }
        
        ""
    }
}

#[no_mangle]
pub extern "C" fn plugin_info() -> *const u8 {
    let json = r#"{"name":"Boop Counter","version":"0.3.0","description":"Counts boops and sends chatbox messages"}"#;
    write_string(json)
}

// Fixed-size JSON builder (no allocator in this plugin).
struct JsonBuf {
    data: [u8; 4096],
    pos: usize,
}

impl JsonBuf {
    fn new() -> Self {
        Self { data: [0; 4096], pos: 0 }
    }

    fn push(&mut self, s: &str) {
        let b = s.as_bytes();
        let n = b.len().min(self.data.len() - self.pos);
        self.data[self.pos..self.pos + n].copy_from_slice(&b[..n]);
        self.pos += n;
    }

    /// Push user text inside a JSON string, escaping quotes and backslashes
    /// so a message like `say "hi"` can't break the UI JSON.
    fn push_escaped(&mut self, s: &str) {
        for &c in s.as_bytes() {
            match c {
                b'"' => self.push("\\\""),
                b'\\' => self.push("\\\\"),
                0..=0x1f => self.push(" "),
                _ => {
                    if self.pos < self.data.len() {
                        self.data[self.pos] = c;
                        self.pos += 1;
                    }
                }
            }
        }
    }

    fn push_u32(&mut self, n: u32) {
        let mut buf = [0u8; 10];
        let s = u32_to_str(n, &mut buf);
        let len = s.len();
        let mut tmp = [0u8; 10];
        tmp[..len].copy_from_slice(s.as_bytes());
        self.push(unsafe { core::str::from_utf8_unchecked(&tmp[..len]) });
    }

    fn as_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.data[..self.pos]) }
    }
}

#[no_mangle]
pub extern "C" fn plugin_ui_config() -> *const u8 {
    unsafe {
        let mut b = JsonBuf::new();

        b.push(r#"{"title":"Boop Counter","elements":["#);

        // Live counters (drawn by the app as stat tiles)
        b.push(r#"{"LiveValue":{"label":"Today","config_key":"today_boops"}},"#);
        b.push(r#"{"LiveValue":{"label":"Total","config_key":"total_boops"}},"#);

        // Quiet mode
        b.push(r#"{"Toggle":{"id":"quiet_mode","label":"Quiet mode","value":"#);
        b.push(if QUIET_MODE { "true" } else { "false" });
        b.push(r#"}},{"Label":{"text":"Boops are still counted, but nothing is posted to the chatbox automatically."}},"#);
        b.push(r#"{"Separator":null},"#);

        // OSC address
        b.push(r#"{"Label":{"text":"OSC Configuration"}},"#);
        b.push(r#"{"TextInput":{"id":"boop_input","label":"Boop Input:","default_value":""#);
        b.push_escaped(get_boop_input_addr());
        b.push(r#"","placeholder":"OSC address"}},{"Separator":null},"#);

        // Custom messages table
        b.push(r#"{"Label":{"text":"Custom Messages (shown when today's count contains the number)"}},"#);
        b.push(r#"{"Table":{"columns":["Message","Number"],"empty_text":"No custom messages yet","rows":["#);
        for i in 0..CUSTOM_MESSAGE_COUNT {
            let (number, msg_bytes, msg_len) = CUSTOM_MESSAGES[i];
            if i > 0 {
                b.push(",");
            }
            b.push(r#"{"cells":[""#);
            b.push_escaped(core::str::from_utf8_unchecked(&msg_bytes[..msg_len]));
            b.push(r#"",""#);
            b.push_u32(number);
            // Button id uses the array INDEX, not the number
            b.push(r#""],"button":{"id":"delete_idx_"#);
            b.push_u32(i as u32);
            b.push(r#"","label":"Delete"}}"#);
        }
        b.push("]}},");

        // Add new message
        b.push(r#"{"Separator":null},{"Label":{"text":"Add New Message:"}},"#);
        b.push(r#"{"TextInput":{"id":"new_msg_text","label":"Message:","default_value":"","placeholder":"Max 20 chars"}},"#);
        b.push(r#"{"TextInput":{"id":"new_msg_num","label":"Number:","default_value":"","placeholder":"e.g. 69"}},"#);

        // Actions
        b.push(r#"{"Separator":null},{"Button":{"id":"send_msg","label":"Send Boop Message"}},"#);
        b.push(r#"{"Button":{"id":"reset_today","label":"Reset Today Boops (undo test boops)"}}]}"#);

        write_string(b.as_str())
    }
}

#[no_mangle]
pub extern "C" fn plugin_load_config() {
    log("=== plugin_load_config() called ===");
    
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
    
    // Load quiet mode
    if let Some(q) = load_config_value("quiet_mode") {
        unsafe {
            QUIET_MODE = q == "true";
        }
    }

    // Load custom messages
    load_custom_messages();
    log("Custom messages loaded");
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
            } else if event_str.contains(r#""delete_idx_"#) {
                log("Delete button clicked");
                
                // Delete message by array INDEX
                let pattern = r#""delete_idx_"#;
                if let Some(start) = event_str.find(pattern) {
                    // Was `start + 13`, one past the pattern, which skipped the
                    // first digit, so every Delete removed message 0.
                    let idx_start = start + pattern.len();
                    if let Some(idx_end) = event_str[idx_start..].find('"') {
                        let idx_str = &event_str[idx_start..idx_start + idx_end];
                        let idx = str_to_u32(idx_str) as usize;
                        
                        if idx < CUSTOM_MESSAGE_COUNT {
                            // Log what we're deleting
                            let (number, msg_bytes, msg_len) = &CUSTOM_MESSAGES[idx];
                            let msg_str = core::str::from_utf8_unchecked(&msg_bytes[..*msg_len]);
                            
                            let mut log_buf = [0u8; 128];
                            let mut log_pos = 0;
                            let prefix = b"Deleted message at index ";
                            log_buf[log_pos..log_pos + prefix.len()].copy_from_slice(prefix);
                            log_pos += prefix.len();
                            
                            let mut idx_buf = [0u8; 10];
                            let idx_str_log = u32_to_str(idx as u32, &mut idx_buf);
                            log_buf[log_pos..log_pos + idx_str_log.len()].copy_from_slice(idx_str_log.as_bytes());
                            log_pos += idx_str_log.len();
                            
                            let mid = b": ";
                            log_buf[log_pos..log_pos + mid.len()].copy_from_slice(mid);
                            log_pos += mid.len();
                            
                            log_buf[log_pos..log_pos + msg_str.len()].copy_from_slice(msg_str.as_bytes());
                            log_pos += msg_str.len();
                            
                            let num_part = b" (";
                            log_buf[log_pos..log_pos + num_part.len()].copy_from_slice(num_part);
                            log_pos += num_part.len();
                            
                            let mut num_buf = [0u8; 10];
                            let num_str = u32_to_str(*number, &mut num_buf);
                            log_buf[log_pos..log_pos + num_str.len()].copy_from_slice(num_str.as_bytes());
                            log_pos += num_str.len();
                            
                            log_buf[log_pos] = b')';
                            log_pos += 1;
                            
                            let log_msg = core::str::from_utf8_unchecked(&log_buf[..log_pos]);
                            log(log_msg);
                            
                            // Shift all messages after this one down
                            for i in idx..CUSTOM_MESSAGE_COUNT - 1 {
                                CUSTOM_MESSAGES[i] = CUSTOM_MESSAGES[i + 1];
                            }
                            CUSTOM_MESSAGE_COUNT -= 1;
                            
                            save_custom_messages();
                            log("Message deleted successfully");
                        } else {
                            log("Invalid index - message not found!");
                        }
                    }
                }
            }
        }
        
        // Quiet mode switch (sent immediately when flipped)
        if event_str.contains(r#""ToggleChanged""#) && event_str.contains(r#""quiet_mode""#) {
            QUIET_MODE = event_str.contains(r#""value":true"#);
            save_config_value("quiet_mode", if QUIET_MODE { "true" } else { "false" });
            log(if QUIET_MODE {
                "Quiet mode ON: boops are counted but not posted to the chatbox"
            } else {
                "Quiet mode OFF: boops are posted to the chatbox again"
            });
        }

        // Check for ApplySettings (this is when Apply Changes or Save Message is clicked)
        if event_str.contains("ApplySettings") {
            // Look for: ["boop_input","VALUE"]
            if let Some(start) = event_str.find(r#"["boop_input",""#) {
                let search_start = start + 15;
                if let Some(end) = event_str[search_start..].find(r#""]"#) {
                    let addr = &event_str[search_start..search_start + end];
                    
                    // Log what we're about to save
                    let mut log_buf = [0u8; 256];
                    let mut log_pos = 0;
                    let prefix = b"Saving boop_input_address: '";
                    log_buf[log_pos..log_pos + prefix.len()].copy_from_slice(prefix);
                    log_pos += prefix.len();
                    log_buf[log_pos..log_pos + addr.len()].copy_from_slice(addr.as_bytes());
                    log_pos += addr.len();
                    log_buf[log_pos] = b'\'';
                    log_pos += 1;
                    log(core::str::from_utf8_unchecked(&log_buf[..log_pos]));
                    
                    let len = addr.len().min(127);
                    
                    // Save to memory
                    BOOP_INPUT_ADDR[..len].copy_from_slice(&addr.as_bytes()[..len]);
                    BOOP_INPUT_LEN = len;
                    
                    // Save to config file
                    save_config_value("boop_input_address", addr);
                }
            }
            
            // Check for new message fields
            let mut new_msg_text: &str = "";
            let mut new_msg_num: u32 = 0;
            
            if let Some(start) = event_str.find(r#"["new_msg_text",""#) {
                let search_start = start + 17;
                if let Some(end) = event_str[search_start..].find(r#""]"#) {
                    new_msg_text = &event_str[search_start..search_start + end];
                }
            }
            
            if let Some(start) = event_str.find(r#"["new_msg_num",""#) {
                let search_start = start + 16;
                if let Some(end) = event_str[search_start..].find(r#""]"#) {
                    let num_str = &event_str[search_start..search_start + end];
                    new_msg_num = str_to_u32(num_str);
                }
            }
            
            // If both fields have values and we have space, add the message
            if !new_msg_text.is_empty() && new_msg_num > 0 && CUSTOM_MESSAGE_COUNT < 10 {
                let msg_len = new_msg_text.len().min(20);
                let mut msg_bytes = [0u8; 20];
                msg_bytes[..msg_len].copy_from_slice(&new_msg_text.as_bytes()[..msg_len]);
                
                CUSTOM_MESSAGES[CUSTOM_MESSAGE_COUNT] = (new_msg_num, msg_bytes, msg_len);
                CUSTOM_MESSAGE_COUNT += 1;
                
                save_custom_messages();
                
                // Log what was added
                let mut log_buf = [0u8; 128];
                let mut log_pos = 0;
                let prefix = b"Added message: ";
                log_buf[log_pos..log_pos + prefix.len()].copy_from_slice(prefix);
                log_pos += prefix.len();
                log_buf[log_pos..log_pos + new_msg_text.len()].copy_from_slice(new_msg_text.as_bytes());
                log_pos += new_msg_text.len();
                let mid = b" (";
                log_buf[log_pos..log_pos + mid.len()].copy_from_slice(mid);
                log_pos += mid.len();
                let mut num_buf = [0u8; 10];
                let num_str = u32_to_str(new_msg_num, &mut num_buf);
                log_buf[log_pos..log_pos + num_str.len()].copy_from_slice(num_str.as_bytes());
                log_pos += num_str.len();
                log_buf[log_pos] = b')';
                log_pos += 1;
                let log_msg = core::str::from_utf8_unchecked(&log_buf[..log_pos]);
                log(log_msg);
                
                // Log total count
                let mut count_buf = [0u8; 64];
                let mut count_pos = 0;
                let count_prefix = b"Total messages: ";
                count_buf[count_pos..count_pos + count_prefix.len()].copy_from_slice(count_prefix);
                count_pos += count_prefix.len();
                let mut cnt_buf = [0u8; 10];
                let cnt_str = u32_to_str(CUSTOM_MESSAGE_COUNT as u32, &mut cnt_buf);
                count_buf[count_pos..count_pos + cnt_str.len()].copy_from_slice(cnt_str.as_bytes());
                count_pos += cnt_str.len();
                let count_msg = core::str::from_utf8_unchecked(&count_buf[..count_pos]);
                log(count_msg);
            } else if !new_msg_text.is_empty() || new_msg_num > 0 {
                // One field is filled but not both
                if new_msg_text.is_empty() {
                    log("Cannot add message: message text is empty");
                } else if new_msg_num == 0 {
                    log("Cannot add message: number is 0 or empty");
                } else if CUSTOM_MESSAGE_COUNT >= 10 {
                    log("Cannot add message: maximum 10 messages reached");
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
            
            // Send chatbox message with 2-second cooldown (20 ticks at 100ms),
            // unless quiet mode is on.
            if QUIET_MODE {
                log("Quiet mode: boop counted, chatbox skipped");
            } else if TICK_COUNT - LAST_CHATBOX_SEND >= 20 {
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
        // The host copies the string out right away and never holds two at
        // once, so reuse the heap from the start instead of leaking. Without
        // this, every Apply/Delete (which re-requests the UI config) eats
        // ~1-4 KB until the 64 KB heap overflows into other statics.
        HEAP_POS = 0;
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