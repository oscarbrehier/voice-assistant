use anyhow::Context;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

pub struct STTService {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl STTService {
    pub async fn new(script_dir: PathBuf) -> anyhow::Result<Self> {
        let script_path = script_dir.join("stt_service.py");

        let mut process = Command::new("python")
            .arg(script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("failed to start python stt service")?;

        let stdout = process.stdout.take().context("Failed to get stdout")?;

        let stdin = process.stdin.take().context("Failed to get stdin")?;

        let mut stdout = BufReader::new(stdout);

        let mut ready = String::new();
        stdout.read_line(&mut ready).await?;

        if !ready.contains("READY") {
            anyhow::bail!("STT service failed to start: {}", ready);
        }

        anyhow::Ok(Self {
            process,
            stdin,
            stdout,
        })
    }

    pub async fn transcribe(&mut self, audio: &[f32]) -> anyhow::Result<String> {
        if audio.is_empty() {
            return Ok("".to_string());
        }

        match self.process.try_wait() {
            Ok(Some(status)) => {
                anyhow::bail!("STT process exited with status: {}", status);
            }
            Ok(None) => {}
            Err(e) => {
                anyhow::bail!("Failed to check STT process status: {}", e);
            }
        }

        self.stdin
            .write_all(format!("AUDIO {}\n", audio.len()).as_bytes())
            .await
            .context("Failed to write audio len")?;

        self.stdin.flush().await?;

        let mut bytes = Vec::with_capacity(audio.len() * 4);
        for &samples in audio {
            bytes.extend_from_slice(&samples.to_le_bytes());
        }

        const CHUNK_SIZE: usize = 8192;
        for chunk in bytes.chunks(CHUNK_SIZE) {
            self.stdin.write_all(chunk).await?;
            tokio::time::sleep(Duration::from_micros(50)).await;
        }

        self.stdin.flush().await?;

        let mut result = String::new();

        timeout(Duration::from_secs(10), self.stdout.read_line(&mut result))
            .await
            .context("STT service timeout, got no response within 10s")?
            .context("Failed to read STT output")?;

        Ok(result.trim().to_string())
    }

    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        let _ = self.stdin.write_all(b"QUIT\n").await;
        let _ = self.stdin.flush().await;

        timeout(Duration::from_secs(5), self.process.wait())
            .await
            .context("Timeout waiting for STT service shutdown")
            .context("Failed to wait for process")?;

        Ok(())
    }
}

impl Drop for STTService {
    fn drop(&mut self) {
        let pid = self.process.id();

        if let Some(pid) = pid {
            std::thread::spawn(move || {
                #[cfg(unix)]
                {
                    use nix::sys::signal::{self, Signal};
                    use nix::unistd::Pid;

                    let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                }
                #[cfg(windows)]
                {
                    use std::process::Command;

                    let _ = Command::new("taskkill")
                        .args(&["/PID", &pid.to_string(), "/F"])
                        .output();
                }
            });
        }
    }
}
