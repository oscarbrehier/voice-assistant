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
        let stdin = self.process.stdin.as_mut().context("failed to get stdin")?;

        writeln!(stdin, "AUDIO {}", audio.len())?;
        stdin.flush()?;

        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(audio.as_ptr() as *const u8, audio.len() * 4) };

        const CHUNK_SIZE: usize = 8192;
        for chunk in bytes.chunks(CHUNK_SIZE) {
            stdin.write_all(chunk)?;
            stdin.flush()?;
            std::thread::sleep(std::time::Duration::from_micros(100));
        }

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
