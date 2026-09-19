use std::{
    io::Write,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn ensure(condition: bool, message: &str) -> Result {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

pub fn run(command: &mut Command) -> Result {
    let status = command.status()?;
    ensure(status.success(), &format!("{command:?}: {status}"))
}

pub fn output(command: &mut Command) -> Result<String> {
    let result = command.stderr(Stdio::inherit()).output()?;
    ensure(
        result.status.success(),
        &format!("{command:?}: {}", result.status),
    )?;
    Ok(String::from_utf8(result.stdout)?.trim().into())
}

pub fn filter(command: &mut Command, input: &[u8]) -> Result<Vec<u8>> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut stdin = child.stdin.take().unwrap();
    let result = thread::scope(|scope| {
        let writer = scope.spawn(move || stdin.write_all(input));
        let result = child.wait_with_output();
        writer.join().expect("stdin writer panicked")?;
        result
    })?;
    ensure(
        result.status.success(),
        &format!("{command:?}: {}", result.status),
    )?;
    Ok(result.stdout)
}

pub fn timed(command: &mut Command, seconds: u64) -> Result {
    let mut child = command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Some(status) = child.try_wait()? {
            return ensure(status.success(), &format!("{command:?}: {status}"));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{command:?}: timed out after {seconds}s").into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}
