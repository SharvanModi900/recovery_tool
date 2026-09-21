//! Standalone remote agent receiver server for deployment.
//!
//! Run with:
//!   PORT=7799 STORAGE_DIR=./data remote_agent_server

use recovery_core::remote_agent;
use std::env;
use std::path::PathBuf;

fn main() {
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7799);

    let storage_dir: PathBuf = env::var("STORAGE_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./data"));

    println!("🚀 RecoverKit Remote Agent Server");
    println!("   Port: {}", port);
    println!("   Storage: {}", storage_dir.display());
    println!();

    match remote_agent::start_server(port, &storage_dir) {
        Ok(state) => {
            println!("✅ Server started successfully");
            println!("   Listening on http://0.0.0.0:{}", port);
            println!("   Health check: http://0.0.0.0:{}/health", port);
            println!("   Status: http://0.0.0.0:{}/status", port);
            println!("   Upload endpoint: POST http://0.0.0.0:{}/upload", port);
            println!();
            println!("Press Ctrl+C to stop...");

            // Keep the main thread alive
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
                let s = remote_agent::get_server_state(&storage_dir);
                println!("[status] packages_received={}", s.packages.len());
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to start server: {}", e);
            std::process::exit(1);
        }
    }
}
