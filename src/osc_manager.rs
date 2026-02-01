use anyhow::Result;
use rosc::{OscMessage, OscPacket, OscType};
use std::net::UdpSocket;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::thread;

use crate::console::ConsoleLog;

type MessageCallback = Arc<dyn Fn(&str, &OscType) + Send + Sync>;

pub struct OscManager {
    socket: Arc<UdpSocket>,
    target_address: String,
    console: Arc<RwLock<ConsoleLog>>,
    listeners: Arc<RwLock<HashMap<String, Vec<MessageCallback>>>>,
}

impl OscManager {
    pub fn new(bind_address: &str, target_address: &str, console: Arc<RwLock<ConsoleLog>>) -> Result<Self> {
        let socket = UdpSocket::bind(bind_address)?;
        socket.set_nonblocking(true)?;
        let socket = Arc::new(socket);
        
        console.write().log_info(&format!("OSC bound to {}", bind_address));
        console.write().log_info(&format!("OSC target: {}", target_address));
        
        let listeners = Arc::new(RwLock::new(HashMap::new()));
        
        // Start receiver thread
        let socket_clone = socket.clone();
        let listeners_clone = listeners.clone();
        let console_clone = console.clone();
        
        thread::spawn(move || {
            Self::receive_loop(socket_clone, listeners_clone, console_clone);
        });
        
        Ok(Self {
            socket,
            target_address: target_address.to_string(),
            console,
            listeners,
        })
    }
    
    fn receive_loop(
        socket: Arc<UdpSocket>,
        listeners: Arc<RwLock<HashMap<String, Vec<MessageCallback>>>>,
        console: Arc<RwLock<ConsoleLog>>,
    ) {
        let mut buf = [0u8; rosc::decoder::MTU];
        
        loop {
            match socket.recv_from(&mut buf) {
                Ok((size, _addr)) => {
                    let packet = match rosc::decoder::decode_udp(&buf[..size]) {
                        Ok((_, packet)) => packet,
                        Err(e) => {
                            console.write().log_error(&format!("Failed to decode OSC packet: {}", e));
                            continue;
                        }
                    };
                    
                    Self::handle_packet(packet, &listeners, &console);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No data available, sleep briefly
                    thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => {
                    console.write().log_error(&format!("OSC receive error: {}", e));
                }
            }
        }
    }
    
    fn handle_packet(
        packet: OscPacket,
        listeners: &Arc<RwLock<HashMap<String, Vec<MessageCallback>>>>,
        console: &Arc<RwLock<ConsoleLog>>,
    ) {
        match packet {
            OscPacket::Message(msg) => {
                Self::handle_message(msg, listeners, console);
            }
            OscPacket::Bundle(bundle) => {
                for packet in bundle.content {
                    Self::handle_packet(packet, listeners, console);
                }
            }
        }
    }
    
    fn handle_message(
        msg: OscMessage,
        listeners: &Arc<RwLock<HashMap<String, Vec<MessageCallback>>>>,
        console: &Arc<RwLock<ConsoleLog>>,
    ) {
        let listeners_read = listeners.read();
        
        if let Some(callbacks) = listeners_read.get(&msg.addr) {
            // This address has listeners - log it AND call callbacks
            for arg in &msg.args {
                for callback in callbacks {
                    callback(&msg.addr, arg);
                }
            }
            
            // Log to console (shows in Log tab because plugin is using it)
            let value_str = format!("{:?}", msg.args);
            console.write().log_osc_received(&msg.addr, &value_str);
        } else {
            // No listeners - only update active addresses (shows in Active Addresses tab only)
            let value_str = format!("{:?}", msg.args);
            console.write().update_active_address(&msg.addr, &value_str);
        }
    }
    
    pub fn register_listener<F>(&self, address: String, callback: F)
    where
        F: Fn(&str, &OscType) + Send + Sync + 'static,
    {
        let mut listeners = self.listeners.write();
        listeners
            .entry(address.clone())
            .or_insert_with(Vec::new)
            .push(Arc::new(callback));
        
        self.console.write().log_info(&format!("Registered OSC listener for: {}", address));
    }
    
    pub fn unregister_all_listeners(&self, address: &str) {
        let mut listeners = self.listeners.write();
        listeners.remove(address);
        
        self.console.write().log_info(&format!("Unregistered OSC listeners for: {}", address));
    }
    
    pub fn send_float(&self, address: &str, value: f32) -> Result<()> {
        let msg = OscMessage {
            addr: address.to_string(),
            args: vec![OscType::Float(value)],
        };
        
        let packet = OscPacket::Message(msg);
        let buf = rosc::encoder::encode(&packet)?;
        
        self.socket.send_to(&buf, &self.target_address)?;
        
        // Log sent command
        self.console.write().log_osc_sent(address, &format!("{}", value));
        
        Ok(())
    }
    
    pub fn send_string(&self, address: &str, value: &str) -> Result<()> {
        let msg = OscMessage {
            addr: address.to_string(),
            args: vec![OscType::String(value.to_string())],
        };
        
        let packet = OscPacket::Message(msg);
        let buf = rosc::encoder::encode(&packet)?;
        
        self.socket.send_to(&buf, &self.target_address)?;
        
        Ok(())
    }
    
    // VRChat chatbox: /chatbox/input [string message] [bool send_immediately]
    // send_immediately=true -> sends message directly to chatbox
    // send_immediately=false -> opens keyboard with message pre-filled
    pub fn send_chatbox(&self, message: &str, send_immediately: bool) -> Result<()> {
        let msg = OscMessage {
            addr: "/chatbox/input".to_string(),
            args: vec![
                OscType::String(message.to_string()),
                OscType::Bool(send_immediately),
            ],
        };
        
        let packet = OscPacket::Message(msg);
        let buf = rosc::encoder::encode(&packet)?;
        
        self.socket.send_to(&buf, &self.target_address)?;
        
        // Log sent command
        self.console.write().log_osc_sent("/chatbox/input", &format!("\"{}\" (immediate: {})", message, send_immediately));
        
        Ok(())
    }
}