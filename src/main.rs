pub mod actions;
pub mod audio;
pub mod devices;
mod engine;
pub mod globals;

use crate::engine::engine;
use openaction::global_events::set_global_event_handler;
use openaction::{OpenActionResult, register_action, run};
use std::env;
use std::time::Duration;
use tokio::select;
use tokio::time::interval;

#[tokio::main]
async fn main() -> OpenActionResult<()> {
    {
        use simplelog::*;
        let level = LevelFilter::Debug;
        let mode = TerminalMode::Stdout;
        let colour = ColorChoice::Auto;

        // We need to ignore a couple of packages log output so create a builder.
        let mut config = ConfigBuilder::new();
        config.add_filter_ignore_str("pulseaudio");
        config.add_filter_ignore_str("symphonia");

        let logger = TermLogger::init(level, config.build(), mode, colour);
        if let Err(e) = logger {
            eprintln!("Logger initialisation Failed: {}", e);
        }
    }
    set_global_event_handler(&globals::GLOBALS);

    register_action(actions::pad::PadAction).await;

    // Run the Ticker
    tokio::spawn(ticker());

    run(env::args().collect()).await
}

pub async fn ticker() {
    // Timers to Tick
    let mut blink_tick = interval(Duration::from_millis(250));
    let mut clean_tick = interval(Duration::from_millis(20));

    // Whether a blinking button is currently on or off
    let mut blink = false;
    loop {
        select! {
            _ = blink_tick.tick() => {
                blink = !blink
            }
            _ = clean_tick.tick() => {
                engine().cleanup()
            }
        }
    }
}
