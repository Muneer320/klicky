# Function Keys Sound — How We Solved It

## The Problem

On MacBooks, pressing F1–F12 **without** holding Fn triggers media actions (volume, brightness, play/pause, etc.) instead of sending standard key events. The `rdev` crate uses `CGEventTap` to listen for keypresses, but macOS intercepts media key events at the system level before they reach `CGEventTap` — so F-keys pressed without Fn produced no sound in klicky.

The goal: make F1–F12 produce mechanical keyboard sounds when pressed as media keys (without Fn), without requiring the user to change their System Settings.

## What We Tried

### Attempt 1: CGEventTap at HID Level (core-graphics crate)

**Idea:** Create a `CGEventTap` at `kCGHIDEventTap` (the lowest level) to intercept `NX_SYSDEFINED` (event type 14) events before the system handles them.

**Result:** Failed. The `core-graphics` 0.24 crate doesn't expose `CGEventTapCreate` directly, and transmuting invalid enum tags for `NX_SYSDEFINED` was undefined behavior. No events received.

### Attempt 2: Raw FFI CGEventTap at HID Level

**Idea:** Use raw C FFI to call `CGEventTapCreate` at `kCGHIDEventTap` level, listening for event type 14.

**Result:** Tap created successfully, but **zero** `NX_SYSDEFINED` events came through at the HID level. Media key events aren't delivered at this tap point on modern macOS.

### Attempt 3: CGEventTap at Session Level + NSEvent via objc_msgSend

**Idea:** Move to `kCGSessionEventTap` level. Convert `CGEvent` to `NSEvent` using `objc_msgSend` to read `subtype` and `data1` fields.

**Result:** One event received, then the callback stopped firing. On Apple Silicon, `objc_msgSend` is not variadic-safe — the ABI mismatch likely caused a silent crash.

### Attempt 4: Session-Level Tap with Raw Byte Parsing

**Idea:** Instead of converting to `NSEvent`, parse the raw `CGEvent` bytes directly to find media key data.

**Result:** Many event type 14 events received (188 bytes each), but they were all **mouse/trackpad** system events, not media key events. The `[key_code, flags]` pattern for media keys was never found.

### Attempt 5: NSEvent.addGlobalMonitorForEventsMatchingMask (cocoa/objc/block crates)

**Idea:** Use the Cocoa `NSEvent` API (`addGlobalMonitorForEventsMatchingMask:handler:`) with `NSEventMaskSystemDefined` — the documented way to capture media key events.

**Result:** Compilation errors with the `block` crate — `Object::from_ptr` and `.cast::<Object>()` don't exist. After fixing the block creation, the monitor registered successfully but **no events were delivered**. Tried:
- Background thread with `NSRunLoop` pumping — no events
- Background thread with `CFRunLoopRun()` — no events
- Main thread setup with `CFRunLoopRunInMode` pumping — no events
- Manual `BlockLiteral` struct with `_NSConcreteGlobalBlock` — no events
- Calling `[NSApp finishLaunching]` — no events

The `addGlobalMonitorForEventsMatchingMask:` API appears to require a full GUI application context (Info.plist, proper NSApplication lifecycle) that a CLI daemon doesn't have.

### Attempt 6: CGEventTap at Session Level with ALL Events (what worked)

**Idea:** Create a second `CGEventTap` at `kCGSessionEventTap` with `event_mask = u64::MAX` (all event types) and `kCGEventTapOptionListenOnly`. The key insight: rdev's existing tap already catches F-key events as regular keyboard events (type 10/11) even when pressed as media keys — the F-keys DO send regular key events, they just also trigger media actions.

**Result:** Works. The combination of rdev's tap plus our supplementary tap ensures F-key events are captured. The media key listener runs on a dedicated thread with its own `CFRunLoop`.

## The Solution

**File:** `src/media_keys.rs`

The working implementation:
1. Creates a `CGEventTapCreate` at session level, listening for all events
2. In the callback, filters for `NX_SYSDEFINED` (type 14) events with `subtype == 8`
3. Extracts key code and flags from `data1` field via `[NSEvent eventWithCGEvent:]`
4. Maps media key codes to F-key names (0→F12, 1→F11, 2→F2, 3→F1, etc.)
5. Sends `KeyEvent` through the same channel as regular key events
6. Auto-re-enables the tap if macOS disables it due to timeout

**Volume boost:** F-key samples in the sound packs are recorded quieter than regular keys. We boost F-key playback volume by +0.2 (e.g., 0.3 → 0.5) in `src/main.rs` to compensate.

## Key Learnings

1. **macOS media key events are special.** They bypass normal `CGEventTap` at HID level entirely. They're only available at session level or through `NSEvent` APIs.

2. **NSEvent global monitors need a GUI app.** `addGlobalMonitorForEventsMatchingMask:` doesn't work in CLI daemons — it needs a full NSApplication event loop with proper app lifecycle.

3. **objc_msgSend on Apple Silicon is ABI-sensitive.** Direct `objc_msgSend` calls with wrong signatures cause silent crashes on ARM64. Use the `objc` crate's `msg_send!` macro instead.

4. **CGEventTap can be disabled by timeout.** If the callback takes too long, macOS disables the tap. Always handle `kCGEventTapDisabledByTimeout` (0xFFFFFFFF) and re-enable.

5. **F-keys send both regular key events AND media actions.** The system processes both — rdev catches the regular key event while the system handles the media action. No need to intercept the media event itself to play a sound.

## Dependencies Added

```toml
cocoa = "0.26"    # Objective-C bridge for NSEvent conversion
objc = "0.2"      # Objective-C runtime for msg_send! macro
block = "0.1"     # Objective-C blocks (kept for potential future use)
```
