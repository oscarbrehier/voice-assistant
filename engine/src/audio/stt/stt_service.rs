use anyhow::Context;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};

pub struct STTService {
    process: Child,
    reader: BufReader<ChildStdout>,
}

impl STTService {
    pub fn new(script_dir: PathBuf) -> anyhow::Result<Self> {
        let script_path = script_dir.join("stt_service.py");

        let mut process = Command::new("python")
            .arg(script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("failed to start python stt service")?;

        let stdout = process.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);

        let mut ready = String::new();
        reader.read_line(&mut ready)?;

        if !ready.contains("READY") {
            anyhow::bail!("STT service failed to start");
        }

        anyhow::Ok(Self { process, reader })
    }

    pub fn transcribe(&mut self, audio: &[f32]) -> anyhow::Result<String> {
        if audio.is_empty() {
            return Ok("".to_string());
        }

        let stdin = self.process.stdin.as_mut().context("failed to get stdin")?;

        writeln!(stdin, "AUDIO {}", audio.len())?;
        stdin.flush()?;

        let mut bytes = Vec::with_capacity(audio.len() * 4);
        for &samples in audio {
            bytes.extend_from_slice(&samples.to_le_bytes());
        }

        const CHUNK_SIZE: usize = 8192;
        for chunk in bytes.chunks(CHUNK_SIZE) {
            stdin.write_all(chunk)?;
            std::thread::sleep(std::time::Duration::from_micros(50));
        }

        stdin.flush()?;

        let mut result = String::new();
        self.reader.read_line(&mut result)?;

        Ok(result.trim().to_string())
    }
}

impl Drop for STTService {
    fn drop(&mut self) {
        if let Some(stdin) = self.process.stdin.as_mut() {
            let _ = writeln!(stdin, "QUIT");
        }
        let _ = self.process.wait();
    }
}
