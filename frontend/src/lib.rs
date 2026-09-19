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
pub mod state;
mod terminal;
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
            println!("muse 0.1.0 (protocol {})", generated::API_VERSION);
            return Ok(0);
        }
        terminal::run(channel, &args)
    });
    match result {
        Ok(Ok(code)) => code,
        Ok(Err(error)) => {
            eprintln!("{error}");
            1
        }
        Err(_) => {
            eprintln!("Could not start the interface");
            2
        }
    }
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
