use std::time::Duration;

use tokio::time::sleep;

use crate::{integrations::system::fetch_system_snapshot, state::SharedContext};

pub async fn run_monitoring_loop(state: SharedContext) {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    
    loop {
        sleep(Duration::from_millis(2000)).await;
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        
        if let Ok(new_data) = fetch_system_snapshot(&sys) {
            let mut lock = state.telemetry.write();
            *lock = new_data;
        }

    }
}
