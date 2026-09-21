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

fn attributes(fd: i32) -> io::Result<libc::termios> {
    let mut value = MaybeUninit::uninit();
    // SAFETY: tcgetattr initializes this termios for a live PTY descriptor.
    if unsafe { libc::tcgetattr(fd, value.as_mut_ptr()) } != 0 {
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
    let state = tempfile::tempdir()?;
    for name in ["ui.json", "library.json"] {
        fs::write(state.path().join(name), "keep user data")?;
    }
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("__pty-child")
        .arg(binary)
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
    let mut child = command.spawn().map_err(|e| format!("PTY spawn: {e}"))?;
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
                        unsafe { libc::kill(native_pid(&output)?, signal) } == 0,
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
                    &format!(
                        "Unexpected PTY exit status: {status}; {}",
                        String::from_utf8_lossy(&output[output.len().saturating_sub(300)..])
                    ),
                )?;
                break;
            }
            ensure(
                start.elapsed() < Duration::from_secs(10),
                "PTY exit timed out",
            )?;
            thread::sleep(Duration::from_millis(10));
        }
        ensure(
            contains(&output, b"PTY_RESTORED"),
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
        // The supervisor created this private process group; include its demo child.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
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
    for signal in [
        None,
        Some(libc::SIGINT),
        Some(libc::SIGTERM),
        Some(libc::SIGHUP),
        Some(libc::SIGKILL),
    ] {
        disconnected_session(&binary, signal, false)?;
    }
    disconnected_session(&binary, None, true)?;
    println!(
        "Demo exit, signal cleanup, disconnected/stalled PTYs and writer lock recovery passed"
    );
    Ok(())
}

fn terminal_pair() -> Result<(File, File)> {
    let (mut master_fd, mut slave_fd) = (-1, -1);
    let mut size = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: openpty initializes both descriptors and borrows a valid window size.
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
        "Cannot open disconnected PTY",
    )?;
    let (master, slave) = unsafe { (File::from_raw_fd(master_fd), File::from_raw_fd(slave_fd)) };
    ensure(
        unsafe {
            libc::fcntl(master_fd, libc::F_SETFD, libc::FD_CLOEXEC) == 0
                && libc::fcntl(slave_fd, libc::F_SETFD, libc::FD_CLOEXEC) == 0
                && libc::fcntl(master_fd, libc::F_SETFL, libc::O_NONBLOCK) == 0
        },
        "Cannot configure disconnected PTY",
    )?;
    Ok((master, slave))
}

fn disconnected_session(binary: &Path, signal: Option<i32>, input_only: bool) -> Result {
    let (master, slave) = terminal_pair()?;
    let mut master = Some(master);
    let before = attributes(slave.as_raw_fd())?;
    // Separating output isolates EOF in the input parser from draw/cleanup errors.
    let mut output_terminal = if input_only {
        Some(terminal_pair()?)
    } else {
        None
    };
    let output_slave = output_terminal
        .as_ref()
        .map(|(_, slave)| slave)
        .unwrap_or(&slave);
    let state = tempfile::tempdir()?;
    let path = state.path().join("library.json.lock");
    let owner = File::create(&path)?;
    let lock_fd = owner.as_raw_fd();
    ensure(
        unsafe { libc::flock(lock_fd, libc::LOCK_EX | libc::LOCK_NB) } == 0,
        "Cannot lock test store",
    )?;
    let mut command = Command::new(binary);
    command
        .args(["--demo", "--plain-icons"])
        .env("MUSE_STATE_DIR", state.path())
        .env("TERM", "xterm-256color")
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(output_slave.try_clone()?))
        .stderr(Stdio::from(output_slave.try_clone()?));
    // Inherit only this synthetic lock: demo never opens the user's LibraryStore or MusicKit player.
    // SAFETY: fcntl is async-signal-safe; the parent keeps the descriptor live through spawn.
    unsafe {
        command.pre_exec(move || {
            if libc::fcntl(lock_fd, libc::F_SETFD, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    drop(owner);
    let result = (|| -> Result {
        let mut output = Vec::new();
        let start = Instant::now();
        loop {
            let previous = output.len();
            let output_master = output_terminal
                .as_mut()
                .map(|(master, _)| master)
                .unwrap_or_else(|| master.as_mut().unwrap());
            drain(output_master, &mut output)?;
            if contains(&output[previous.saturating_sub(3)..], b"\x1b[6n") {
                master.as_mut().unwrap().write_all(b"\x1b[1;1R")?;
            }
            if contains(&output, b"\x1b[?2004h") && start.elapsed() > Duration::from_millis(250) {
                break;
            }
            ensure(
                start.elapsed() < Duration::from_secs(5),
                "Disconnected PTY startup timed out",
            )?;
            thread::sleep(Duration::from_millis(10));
        }
        let recovered = File::options().read(true).write(true).open(&path)?;
        ensure(
            unsafe { libc::flock(recovered.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == -1,
            "Demo did not retain the synthetic writer lock",
        )?;
        if let Some(signal) = signal {
            // Keep the master open but stop reading: animated output fills the PTY and blocks draw/flush.
            thread::sleep(Duration::from_secs(1));
            ensure(
                unsafe { libc::kill(child.id() as i32, signal) } == 0,
                "Cannot signal stalled demo",
            )?;
        } else {
            drop(master.take()); // No signal injection: terminal loss alone must initiate shutdown.
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some((master, _)) = output_terminal.as_mut() {
                drain(master, &mut output)?;
            }
            if let Some(status) = child.try_wait()? {
                if signal == Some(libc::SIGKILL) {
                    use std::os::unix::process::ExitStatusExt;
                    ensure(
                        status.signal() == Some(libc::SIGKILL),
                        "Expected abrupt process death",
                    )?;
                } else {
                    ensure(
                        status.code() == Some(128 + signal.unwrap_or(libc::SIGHUP))
                            // A revoked TTY may return an I/O error or a caught Ratatui Drop panic.
                            || (!input_only && signal.is_none() && matches!(status.code(), Some(1 | 2))),
                        &format!("Unexpected disconnected PTY exit: {status}"),
                    )?;
                }
                break;
            }
            ensure(
                Instant::now() < deadline,
                &format!("Disconnected/stalled PTY exit timed out: {signal:?}"),
            )?;
            thread::sleep(Duration::from_millis(10));
        }
        ensure(
            unsafe { libc::flock(recovered.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
            "Exited demo retained the writer lock",
        )?;
        if signal.is_some() && signal != Some(libc::SIGKILL) {
            let after = attributes(slave.as_raw_fd())?;
            ensure(
                before.c_lflag == after.c_lflag
                    && before.c_iflag == after.c_iflag
                    && before.c_cc == after.c_cc,
                "Stalled demo did not restore terminal input",
            )?;
        }
        Ok(())
    })();
    if result.is_err() {
        drop(master.take());
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

fn native_pid(output: &[u8]) -> Result<i32> {
    let text = String::from_utf8_lossy(output);
    let value = text.split("PTY_CHILD=").nth(1).ok_or("Missing PTY child")?;
    Ok(value
        .split_whitespace()
        .next()
        .ok_or("Missing PTY pid")?
        .parse()?)
}

pub fn child(binary: &Path) -> Result<i32> {
    let before = attributes(libc::STDIN_FILENO)?;
    let mut child = Command::new(binary)
        .args(["--demo", "--plain-icons", "--reduced-motion"])
        .current_dir(binary.parent().ok_or("Missing binary directory")?)
        .spawn()?;
    println!("PTY_CHILD={}", child.id());
    let status = child.wait()?;
    // macOS revokes the PTY when its session leader exits. Inspect it while this
    // supervising process still owns the session and the application has exited.
    let after = attributes(libc::STDIN_FILENO)?;
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
    println!("PTY_RESTORED");
    status
        .code()
        .ok_or_else(|| "Demo was terminated without cleanup".into())
}
