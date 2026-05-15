use all_smi::{AllSmi};
use chrono::Local;
use std::{process::Command, thread, time::Duration};
use crate::state::{ProcessSnapshot, Vitals};

pub fn spawn_app(app: String) -> anyhow::Result<()> {
    Command::new(app).output().expect("failed to spawn app");

    Ok(())
}

pub fn fetch_system_snapshot(system: &sysinfo::System) -> anyhow::Result<Vitals> {
    let smi = AllSmi::new()?;

    let _ = smi.get_cpu_info();
    thread::sleep(Duration::from_millis(500));
    
    let cpus = smi.get_cpu_info();
    let cpu_count = cpus.len() as f64;

    let avg_util: f64 = cpus.iter().map(|c| c.utilization as f64).sum::<f64>() / cpu_count;
    let max_temp = cpus.iter().filter_map(|c| c.temperature).max();

    let mem_info = smi.get_memory_info();
    let mem = mem_info.first().ok_or_else(|| anyhow::anyhow!("No memory info detected"))?;

    let used_gb = mem.used_bytes as f64 / 1024.0 / 1024.0 / 1024.0;
    let total_gb = mem.total_bytes as f64 / 1024.0 / 1024.0 / 1024.0;

    let mut procs: Vec<ProcessSnapshot> = system.processes()
        .values()
        .map(|v| ProcessSnapshot { 
            name: v.name().to_string_lossy().into_owned(), 
            cpu_percent: v.cpu_usage(), 
            memory_mb: v.memory() / (1024 * 1024),
        })
        .collect();

    procs.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap_or(std::cmp::Ordering::Equal));
    procs.truncate(5);

    let now = Local::now();
    let timestamp = now.format("%H:%M:%S").to_string();
    
    let telemetry = Vitals {
        cpu_load: avg_util,
        cpu_temperature: max_temp,
        ram_used_gb: (used_gb * 100.0).round() / 100.0,
        ram_total_gb: (total_gb * 100.0).round() / 100.0,
        timestamp,
        top_processes: procs
    };

    Ok(telemetry)
}
