use anyhow::Result;
use tokio::io::AsyncReadExt;
use tracing::info;

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

pub async fn log_stdio<T>(
    mut source: T,
    stdio: StdioType,
) -> Result<()>
where
    T: AsyncReadExt + std::marker::Unpin,
{
    let stdio: &'static str = stdio.into();
    let mut line = String::new();
    loop {
        let mut buf = [0; 512];
        let mut last_newline = 0;
        match source.read(&mut buf[..]).await {
            // No input, the writing end has been closed
            Ok(0) => break,
            Ok(n) => {
                let buf: &str = std::str::from_utf8(&buf[..n])?;
                // If there is output remaining from the latest read call
                // and there is a newline in buf
                if !line.is_empty() && let Some(index) = buf.find('\n') {
                    // print the resulting line
                    info!("[{stdio}] {line}{}", &buf[..index]);
                    line.clear();
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
                    line.push_str(&buf[last_newline..]);
                }
            }
            Err(err) => Err(err)?,
        }
    }

    // No new output here, print the remaining line
    if !line.is_empty() {
        info!("[{stdio}] {line}");
    }

    Ok(())
}
