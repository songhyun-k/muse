use crate::process::{Result, ensure};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    mem::MaybeUninit,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn attributes(file: &File) -> io::Result<libc::termios> {
    let mut value = MaybeUninit::uninit();
    // SAFETY: tcgetattr initializes this termios for a live PTY descriptor.
    if unsafe { libc::tcgetattr(file.as_raw_fd(), value.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { value.assume_init() })
}

fn drain(master: &mut File, output: &mut Vec<u8>) -> io::Result<()> {
    let mut bytes = [0; 16384];
    loop {
        match master.read(&mut bytes) {
            Ok(0) => return Ok(()),
            Ok(n) => output.extend_from_slice(&bytes[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|part| part == needle)
}

fn session(binary: &Path, signal: Option<i32>) -> Result {
    let (mut master_fd, mut slave_fd) = (-1, -1);
    let mut size = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: openpty writes both descriptors, with a valid size and default terminal attributes.
    ensure(
        unsafe {
            libc::openpty(
                &mut master_fd,
                &mut slave_fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        } == 0,
        "Cannot open PTY",
    )?;
    let (mut master, slave) =
        unsafe { (File::from_raw_fd(master_fd), File::from_raw_fd(slave_fd)) };
    // SAFETY: the owned descriptors remain live; neither is leaked into the child after exec.
    ensure(
        unsafe {
            libc::fcntl(master_fd, libc::F_SETFD, libc::FD_CLOEXEC) == 0
                && libc::fcntl(slave_fd, libc::F_SETFD, libc::FD_CLOEXEC) == 0
                && libc::fcntl(master_fd, libc::F_SETFL, libc::O_NONBLOCK) == 0
        },
        "Cannot configure PTY",
    )?;
    let before = attributes(&slave)?;
    let state = tempfile::tempdir()?;
    for name in ["ui.json", "library.json"] {
        fs::write(state.path().join(name), "keep user data")?;
    }
    let mut command = Command::new(binary);
    command
        .args(["--demo", "--plain-icons", "--reduced-motion"])
        .current_dir(binary.parent().ok_or("Missing binary directory")?)
        .env("MUSE_STATE_DIR", state.path())
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("TERM_PROGRAM", "muse-test")
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave.try_clone()?));
    // SAFETY: only async-signal-safe libc calls run between fork and exec.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() == -1 || libc::ioctl(slave_fd, libc::TIOCSCTTY.into(), 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    let result = (|| -> Result {
        let mut output = Vec::new();
        let start = Instant::now();
        let mut sent = false;
        loop {
            let previous = output.len();
            drain(&mut master, &mut output)?;
            if contains(&output[previous.saturating_sub(3)..], b"\x1b[6n") {
                master.write_all(b"\x1b[1;1R")?;
            }
            if !sent
                && start.elapsed() > Duration::from_millis(250)
                && contains(&output, b"\x1b[?2004h")
            {
                if let Some(signal) = signal {
                    ensure(
                        unsafe { libc::kill(child.id() as i32, signal) } == 0,
                        "Cannot interrupt demo",
                    )?;
                } else {
                    master.write_all(b"q")?;
                }
                sent = true;
            }
            if let Some(status) = child.try_wait()? {
                drain(&mut master, &mut output)?;
                ensure(
                    sent && status.code() == Some(signal.map_or(0, |s| 128 + s)),
                    "Unexpected PTY exit status",
                )?;
                break;
            }
            ensure(
                start.elapsed() < Duration::from_secs(10),
                "PTY exit timed out",
            )?;
            thread::sleep(Duration::from_millis(10));
        }
        let after = attributes(&slave)?;
        ensure(
            before.c_iflag == after.c_iflag
                && before.c_oflag == after.c_oflag
                && before.c_cflag == after.c_cflag
                && before.c_lflag == after.c_lflag
                && before.c_cc == after.c_cc
                && before.c_ispeed == after.c_ispeed
                && before.c_ospeed == after.c_ospeed,
            "Terminal attributes were not restored",
        )?;
        for sequence in [
            b"\x1b[?1049l".as_slice(),
            b"\x1b[?25h",
            b"\x1b[?2004l",
            b"\x1b[?1006l",
        ] {
            ensure(contains(&output, sequence), "Missing terminal cleanup")?;
        }
        for name in ["ui.json", "library.json"] {
            ensure(
                fs::read(state.path().join(name))? == b"keep user data",
                "Demo changed saved data",
            )?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

pub fn check(binary: &Path) -> Result {
    let binary = binary.canonicalize()?;
    for signal in [
        None,
        Some(libc::SIGINT),
        Some(libc::SIGTERM),
        Some(libc::SIGHUP),
    ] {
        session(&binary, signal)?;
    }
    println!("Demo exit, signal cleanup and saved-data isolation passed");
    Ok(())
}
