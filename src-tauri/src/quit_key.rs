//! Cmd+Q, handled where AppKit first sees the key.
//!
//! The "Quit BOB" menu item has the Cmd+Q key equivalent and works when clicked, but the keystroke never
//! reached it (nor the page's keydown fallback): something between AppKit's event queue and the menu
//! consumes it. An NSEvent local monitor runs before `sendEvent:`, so it sees the key first, whatever the
//! web view or the responder chain would do with it.
//!
//! Privacy: only key-downs with Cmd are inspected; only the key code is logged, never characters.

use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
use std::ptr::NonNull;

/// True for Cmd+Q with no other modifier (Caps Lock and the numeric-pad / function flags are ignored).
pub fn is_quit(characters_ignoring_modifiers: Option<&str>, flags: NSEventModifierFlags) -> bool {
    let relevant = flags
        & (NSEventModifierFlags::Command
            | NSEventModifierFlags::Shift
            | NSEventModifierFlags::Option
            | NSEventModifierFlags::Control);
    relevant == NSEventModifierFlags::Command
        && characters_ignoring_modifiers.is_some_and(|c| c.eq_ignore_ascii_case("q"))
}

/// Install the monitor. Must run on the main thread (Tauri's `setup` does). Lives for the whole process.
pub fn install(on_quit: impl Fn() + 'static) {
    let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a valid event for the duration of the call.
        let ev = unsafe { event.as_ref() };
        let flags = ev.modifierFlags();
        if flags.contains(NSEventModifierFlags::Command) {
            let chars = ev.charactersIgnoringModifiers().map(|s| s.to_string());
            log::debug!("cmd key down: keycode {}", ev.keyCode());
            if is_quit(chars.as_deref(), flags) {
                log::info!("quit requested with Cmd+Q");
                on_quit();
                return std::ptr::null_mut();
            }
        }
        event.as_ptr()
    });
    // SAFETY: called on the main thread; the handler returns either the event or null, as documented.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
    };
    match monitor {
        Some(m) => std::mem::forget(m),
        None => log::warn!("could not install the Cmd+Q monitor"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_cmd_q_quits() {
        let cmd = NSEventModifierFlags::Command;
        assert!(is_quit(Some("q"), cmd));
        assert!(is_quit(Some("Q"), cmd | NSEventModifierFlags::CapsLock));
        assert!(!is_quit(Some("q"), cmd | NSEventModifierFlags::Shift));
        assert!(!is_quit(Some("q"), cmd | NSEventModifierFlags::Option));
        assert!(!is_quit(Some("q"), cmd | NSEventModifierFlags::Control));
        assert!(!is_quit(Some("q"), NSEventModifierFlags::empty()));
        assert!(!is_quit(Some("w"), cmd));
        assert!(!is_quit(None, cmd));
    }
}
