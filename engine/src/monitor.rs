use std::time::Duration;

use tokio::time::sleep;

use crate::{actions::system::fetch_system_snapshot, state::SharedContext};

pub async fn run_monitoring_loop(state: SharedContext) {
    loop {
        if let Ok(new_data) = fetch_system_snapshot() {
            let mut lock = state.telemetry.write();
            *lock = new_data;
        }

        sleep(Duration::from_millis(2000)).await;
    }
}
