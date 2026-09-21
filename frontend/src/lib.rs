pub mod app;
pub mod art;
pub mod canvas;
pub mod content;
pub mod controls;
pub mod dialogs;
pub mod editing;
pub mod effects;
pub mod generated;
pub mod geometry;
pub mod i18n;
pub mod icons;
pub mod input;
pub mod lists;
pub mod lyrics;
pub mod menus;
pub mod motion;
mod mouse;
pub mod overlays;
pub mod player;
mod preferences;
pub mod requests;
pub mod scene;
mod scheduling;
pub mod settings;
pub mod state;
mod terminal;
mod terminal_lifecycle;
pub mod theme;
pub mod transport;
pub mod wire;

/// # Safety
/// The host must honor contract/transport.h, including pointer validity/lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muse_run(
    bridge: transport::Bridge,
    options: *const u8,
    length: usize,
) -> i32 {
    if options.is_null() || length > 65536 {
        return 2;
    }
    let result = std::panic::catch_unwind(|| {
        // SAFETY: buffers and callbacks are borrowed only until muse_run returns.
        let args: Vec<String> =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(options, length) })
                .map_err(|e| e.to_string())?;
        let channel = unsafe { transport::Channel::new(bridge) }?;
        if args.iter().any(|a| a == "--probe") {
            return probe(channel).map(|()| 0);
        }
        if args.iter().any(|a| a == "--version") {
            println!(
                "muse {} (protocol {})",
                env!("CARGO_PKG_VERSION"),
                generated::API_VERSION
            );
            return Ok(0);
        }
        terminal::run(channel, &args)
    });
    match result {
        Ok(Ok(code)) => code,
        Ok(Err(error)) => {
            terminal_lifecycle::report_error(&error);
            1
        }
        Err(payload) => {
            terminal_lifecycle::report_error(&panic_message(payload.as_ref()));
            2
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-text panic payload");
    // Bound work before sanitizing untrusted text, without slicing UTF-8 bytes.
    let mut chars = message.chars();
    let mut detail = canvas::clean(&chars.by_ref().take(512).collect::<String>());
    if detail.trim().is_empty() {
        detail = "no printable panic message".into();
    }
    if chars.next().is_some() {
        detail.push('…');
    }
    format!("Interface failed (Rust panic): {detail}")
}

fn probe(mut channel: transport::Channel) -> Result<(), String> {
    use generated::{Command, Empty, Request};
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let request = Request {
        version: generated::API_VERSION,
        id: 1,
        command: Command::Snapshot(Empty {}),
    };
    if channel.send(&request)? != transport::Admission::Accepted {
        return Err("Probe rejected".into());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(event) = channel.receive()? {
            if event.id != Some(1) {
                return Err("Probe correlation mismatch".into());
            }
            println!(
                "Swift ↔ Rust contract round-trip OK (protocol {})",
                event.version
            );
            return Ok(());
        }
        if channel.closed {
            return Err("Probe connection closed".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
    Err("Probe timed out".into())
}

#[cfg(test)]
mod tests {
    use super::panic_message;
    use std::panic::{catch_unwind, panic_any};

    #[test]
    fn caught_panics_keep_bounded_terminal_safe_diagnostics() {
        let payload = catch_unwind(|| {
            panic!("invalid 가사\u{1b}[31m!\u{1b}]52;c;secret\u{7}\u{9b}2J\u{202e}\r\n")
        })
        .unwrap_err();
        assert_eq!(
            panic_message(payload.as_ref()),
            "Interface failed (Rust panic): invalid 가사!"
        );
        let payload = catch_unwind(|| panic_any("가".repeat(513))).unwrap_err();
        assert_eq!(
            panic_message(payload.as_ref()),
            format!("Interface failed (Rust panic): {}…", "가".repeat(512))
        );
        let payload = catch_unwind(|| panic_any(19)).unwrap_err();
        assert_eq!(
            panic_message(payload.as_ref()),
            "Interface failed (Rust panic): non-text panic payload"
        );
        assert_eq!(
            panic_message(&"\u{1b}]52;c;secret\u{7}\n"),
            "Interface failed (Rust panic): no printable panic message"
        );
    }
}
