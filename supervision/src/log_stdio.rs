use std::future;

use anyhow::Result;
use tokio::{
    io::AsyncReadExt,
    process::{
        ChildStderr,
        ChildStdout,
    },
    select,
};
use tracing::{
    info,
    warn,
};

pub enum StdioType {
    Stdout,
    Stderr,
}

impl From<StdioType> for &'static str {
    fn from(s: StdioType) -> &'static str {
        match s {
            StdioType::Stdout => "stdout",
            StdioType::Stderr => "stderr",
        }
    }
}

pub fn log_buf(
    previous_line: &mut String,
    buf: &[u8],
    stdio: &'static str,
) -> Result<()> {
    let buf: &str = std::str::from_utf8(buf)?;
    let mut last_newline = 0;
    // If there is output remaining from the latest read call
    // and there is a newline in buf
    if !previous_line.is_empty() && let Some(index) = buf.find('\n') {
                    // print the resulting line
                    info!("[{stdio}] {previous_line}{}", &buf[..index]);
                    previous_line.clear();
                    last_newline = index + 1;
                }

    // iterate all other lines and stop when either the newline found
    // was the last character or the there are no more newlines
    while last_newline != buf.len() && let Some(index) = buf[last_newline..].find('\n') {
                    // the index returned from find refers to new &str buf[last_newline..]
                    // so we need to port it back for buf, by adding last_new_line to it
                    let index = last_newline + index;
                    // print every line
                    info!("[{stdio}] {}", &buf[last_newline..index]);
                    last_newline = index + 1;
                }

    // if there are characters after the last newline, store them
    // so that they can be print in the next loop
    if last_newline != buf.len() {
        previous_line.push_str(&buf[last_newline..]);
    }

    Ok(())
}

pub async fn log_output(
    mut stdout: ChildStdout,
    mut stderr: ChildStderr,
    mut rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    let mut stdout_line = String::new();
    let mut stderr_line = String::new();
    let mut stdout_open = true;
    let mut stderr_open = true;
    loop {
        let mut stdout_buf = [0; 512];
        let mut stderr_buf = [0; 512];
        select! {
            read = async {
                if stdout_open {
                    stdout.read(&mut stdout_buf[..]).await
                } else {
                    future::pending::<()>().await;
                    unreachable!()
                }
            } => {
                // This is always Some
                match read {
                    // No input, the writing end has been closed
                    Ok(0) => {
                        stdout_open = false;
                    }
                    Ok(n) => {
                        if let Err(err) = log_buf(&mut stdout_line, &stdout_buf[..n], "stdout") {
                            warn!("{err}");
                        }
                    }
                    Err(err) => Err(err)?,
                }
            },
            read = async {
                if stderr_open {
                    Some(stderr.read(&mut stderr_buf[..]).await )
                } else {
                    future::pending::<()>().await;
                    unreachable!()
                }
            } => {
                let read = read.unwrap();
                match read {
                    // No input, the writing end has been closed
                    Ok(0) => {
                        stderr_open = false;
                    }
                    Ok(n) => {
                        if let Err(err) = log_buf(&mut stderr_line, &stderr_buf[..n], "stderr") {
                            warn!("{err}");
                        }
                    }
                    Err(err) => Err(err)?,
                }
            }
            _ = &mut rx => {
                break;
            }
        }

        // If both ends are closed, exit out of the loop
        if !stdout_open && !stderr_open {
            break;
        }
    }

    if let Err(err) = log_buf(&mut stdout_line, &[], "stdout") {
        warn!("{err}");
    }

    if let Err(err) = log_buf(&mut stderr_line, &[], "stderr") {
        warn!("{err}");
    }

    Ok(())
}
