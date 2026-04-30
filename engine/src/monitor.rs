use std::{sync::{Arc, RwLock}, time::Duration};

use tokio::time::sleep;

use crate::{actions::system::{fetch_system_snapshot}, state::SharedContext};

pub async fn run_monitoring_loop(state: SharedContext) {
    loop {
        if let Ok(new_data) = fetch_system_snapshot() {

            match state.telemetry.write() {
                Ok(mut lock) => {
                    *lock = new_data;
                }
                Err(e) => {
                    eprintln!("Failed to acquire state lock: {}", e);
                }
            }
            
        }
        
        sleep(Duration::from_millis(2000)).await;
    }
}