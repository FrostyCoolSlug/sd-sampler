pub mod actions;
pub mod globals;
pub mod devices;

use std::env;
use openaction::{run, OpenActionResult, register_action};
use openaction::global_events::set_global_event_handler;

#[tokio::main]
async fn main() -> OpenActionResult<()> {
    {
        use simplelog::*;
        let level = LevelFilter::Debug;
        let config = Config::default();
        let mode = TerminalMode::Stdout;
        let colour = ColorChoice::Auto;

        let logger = TermLogger::init(level, config, mode, colour);
        if let Err(e) = logger {
            eprintln!("Logger initialisation Failed: {}", e);
        }
    }
    set_global_event_handler(&globals::GLOBALS);

    register_action(actions::sample::SampleAction).await;

    run(env::args().collect()).await
}
