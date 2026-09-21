use std::{
    ffi::{CStr, c_void},
    fs::File,
    io::{self, IsTerminal},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    },
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

struct TerminalState {
    original: libc::termios,
    output: File,
    restored: Mutex<bool>,
}

static TERMINAL: OnceLock<TerminalState> = OnceLock::new();

struct Shutdown {
    context: *mut c_void,
    notify: unsafe extern "C" fn(*mut c_void, i32),
}

// SAFETY: the ABI requires a thread-safe callback and process-lifetime context.
unsafe impl Send for Shutdown {}

impl Shutdown {
    fn request(&self, code: i32) {
        unsafe { (self.notify)(self.context, code) };
    }
}

/// # Safety
/// Called once before muse_run. Context/callback must remain valid until process exit;
/// the callback runs on the terminal event thread and must neither block nor unwind.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn muse_prepare_terminal(
    context: *mut c_void,
    notify: Option<unsafe extern "C" fn(*mut c_void, i32)>,
) -> i32 {
    let Some(notify) = notify.filter(|_| !context.is_null()) else {
        return 1;
    };
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return 0;
    }
    i32::from(prepare(Shutdown { context, notify }).is_err())
}

fn prepare(shutdown: Shutdown) -> io::Result<()> {
    let mut original = std::mem::MaybeUninit::uninit();
    let mut name = [0; 1024];
    // SAFETY: both calls initialize their supplied buffers; stdio remains open until exit.
    if unsafe { libc::tcgetattr(libc::STDIN_FILENO, original.as_mut_ptr()) } != 0
        || unsafe { libc::ttyname_r(libc::STDOUT_FILENO, name.as_mut_ptr(), name.len()) } != 0
    {
        return Err(io::Error::last_os_error());
    }
    let path = unsafe { CStr::from_ptr(name.as_ptr()) };
    // A separate open description preserves the shell's stdout flags and never waits for output.
    let output = File::options()
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(std::ffi::OsStr::from_bytes(path.to_bytes()))?;
    TERMINAL
        .set(TerminalState {
            original: unsafe { original.assume_init() },
            output,
            restored: Mutex::new(false),
        })
        .map_err(|_| io::Error::other("Terminal already prepared"))?;
    let descriptor = unsafe { libc::kqueue() };
    if descriptor < 0 {
        return Err(io::Error::last_os_error());
    }
    let queue = unsafe { File::from_raw_fd(descriptor) };
    if unsafe { libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let signal = Arc::new(AtomicUsize::new(0));
    let mut registrations = Vec::new();
    for number in [libc::SIGHUP, libc::SIGINT, libc::SIGTERM] {
        registrations.push(signal_hook::flag::register_usize(
            number,
            signal.clone(),
            number as usize,
        )?);
    }
    let mut changes: Vec<_> = [
        (libc::STDIN_FILENO, libc::EVFILT_READ),
        (libc::SIGHUP, libc::EVFILT_SIGNAL),
        (libc::SIGINT, libc::EVFILT_SIGNAL),
        (libc::SIGTERM, libc::EVFILT_SIGNAL),
    ]
    .into_iter()
    .map(|(ident, filter)| libc::kevent {
        ident: ident as usize,
        filter,
        flags: libc::EV_ADD | libc::EV_CLEAR,
        fflags: 0,
        data: 0,
        udata: std::ptr::null_mut(),
    })
    .collect();
    // SAFETY: kevent borrows initialized change records; no output buffer is requested.
    if unsafe {
        libc::kevent(
            descriptor,
            changes.as_mut_ptr(),
            changes.len() as i32,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    thread::Builder::new()
        .name("terminal-events".into())
        .spawn(move || {
            let code = loop {
                let requested = signal.load(Ordering::Relaxed);
                if requested != 0 {
                    break 128 + requested as i32;
                }
                let mut event = std::mem::MaybeUninit::uninit();
                // No timeout: kernel signal/EOF notifications wake the host even if UI I/O is stuck.
                let count = unsafe {
                    libc::kevent(
                        queue.as_raw_fd(),
                        std::ptr::null(),
                        0,
                        event.as_mut_ptr(),
                        1,
                        std::ptr::null(),
                    )
                };
                if count < 0 {
                    if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    break 1;
                }
                if count == 0 {
                    continue;
                }
                let event = unsafe { event.assume_init() };
                if event.filter == libc::EVFILT_SIGNAL {
                    break 128 + event.ident as i32;
                }
                if event.flags & (libc::EV_EOF | libc::EV_ERROR) != 0 {
                    break 128 + libc::SIGHUP;
                }
            };
            shutdown.request(code);
            for registration in registrations {
                signal_hook::low_level::unregister(registration);
            }
        })?;
    Ok(())
}

pub fn enter_raw_mode() -> io::Result<()> {
    let state = TERMINAL
        .get()
        .ok_or_else(|| io::Error::other("Terminal not prepared"))?;
    let restored = state
        .restored
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if *restored {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Terminal closed",
        ));
    }
    // Serialize only raw-mode setup with restoration, never drawing or terminal reads.
    crossterm::terminal::enable_raw_mode()
}

/// Restore input and attempt terminal reset without stdio locks or blocking writes.
#[unsafe(no_mangle)]
pub extern "C" fn muse_restore_terminal() {
    let Some(state) = TERMINAL.get() else { return };
    {
        let mut restored = state
            .restored
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *restored = true;
        // SAFETY: original was captured before raw mode and remains valid until process exit.
        unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &state.original) };
    }
    let reset = b"\x1b[0m\x1b[?25h\x1b[?2004l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1015l\x1b[?1006l\x1b[?1049l";
    // SAFETY: best-effort write to our private nonblocking descriptor; a lost/full PTY is harmless.
    unsafe { libc::write(state.output.as_raw_fd(), reset.as_ptr().cast(), reset.len()) };
}
