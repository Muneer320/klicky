use std::ffi::c_void;
use std::sync::mpsc::Sender;
use std::time::Instant;

use objc::{class, msg_send, sel, sel_impl};

use crate::listener::KeyEvent;

static mut MEDIA_SENDER: Option<Sender<KeyEvent>> = None;

type CGEventTapProxy = *mut c_void;
type CGEventRef = *mut c_void;
type CFMachPortRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CFStringRef = *const c_void;
type CFAllocatorRef = *const c_void;

extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: extern "C" fn(CGEventTapProxy, u32, CGEventRef, *mut c_void) -> CGEventRef,
        user_info: *mut c_void,
    ) -> CFMachPortRef;

    fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: i64,
    ) -> CFRunLoopSourceRef;

    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRun();
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);

    static kCFRunLoopCommonModes: CFStringRef;
    static kCFAllocatorDefault: CFAllocatorRef;
}

fn media_key_to_name(key_code: i64) -> Option<&'static str> {
    match key_code {
        0 => Some("F12"),
        1 => Some("F11"),
        2 => Some("F2"),
        3 => Some("F1"),
        7 => Some("F10"),
        9 => Some("F3"),
        10 => Some("F4"),
        16 => Some("F8"),
        17 => Some("F9"),
        18 => Some("F7"),
        20 => Some("F5"),
        21 => Some("F6"),
        22 => Some("F5"),
        _ => None,
    }
}

static mut TAP_PORT: CFMachPortRef = std::ptr::null_mut();

extern "C" fn tap_callback(
    _proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    _user_info: *mut c_void,
) -> CGEventRef {
    unsafe {
        // Re-enable tap if it was disabled by timeout
        if event_type == 0xFFFFFFFF {
            if !TAP_PORT.is_null() {
                CGEventTapEnable(TAP_PORT, true);
            }
            return event;
        }

        // NX_SYSDEFINED = 14 — media key events
        if event_type == 14 {
            let ns_event: *mut c_void = msg_send![class!(NSEvent), eventWithCGEvent: event];
            if !ns_event.is_null() {
                let subtype: i16 = msg_send![ns_event as *mut objc::runtime::Object, subtype];
                if subtype == 8 {
                    let data1: i64 = msg_send![ns_event as *mut objc::runtime::Object, data1];
                    let key_code = (data1 >> 16) & 0xFF;
                    let key_flags = (data1 & 0xFF00) >> 8;
                    let is_key_down = (key_flags & 0x0A) != 0 && (key_flags & 0x01) == 0;

                    if is_key_down {
                        if let Some(name) = media_key_to_name(key_code) {
                            if let Some(ref sender) = MEDIA_SENDER {
                                let _ = sender.send(KeyEvent {
                                    key_name: name.to_string(),
                                    timestamp: Instant::now(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    event
}

pub fn start_media_key_listener(sender: Sender<KeyEvent>) {
    std::thread::spawn(move || {
        unsafe {
            MEDIA_SENDER = Some(sender);

            let port = CGEventTapCreate(
                1, // kCGSessionEventTap
                0, // kCGHeadInsertEventTap
                1, // kCGEventTapOptionListenOnly
                u64::MAX,
                tap_callback,
                std::ptr::null_mut(),
            );

            if port.is_null() {
                return;
            }

            TAP_PORT = port;

            let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, port, 0);
            if source.is_null() {
                return;
            }

            let run_loop = CFRunLoopGetCurrent();
            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
            CGEventTapEnable(port, true);
            CFRunLoopRun();
        }
    });
}
