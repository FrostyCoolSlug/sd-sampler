pub mod actions;
pub mod audio;
pub mod devices;
mod engine;
pub mod globals;
pub mod svg;
mod wavecache;

use crate::engine::engine;
use openaction::global_events::set_global_event_handler;
use openaction::{OpenActionResult, register_action, run};
use std::env;
use std::time::Duration;
use tokio::select;
use tokio::time::interval;
use crate::actions::add::draw_all_adds;
use crate::actions::clear::draw_all_clears;
use crate::actions::pad::draw_all_pads;

#[tokio::main]
async fn main() -> OpenActionResult<()> {
    {
        use simplelog::*;
        let level = LevelFilter::Debug;
        let mode = TerminalMode::Stdout;
        let colour = ColorChoice::Never;

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
    register_action(actions::clear::ClearAction).await;
    register_action(actions::add::AddAction).await;

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
                blink = !blink;
                draw_all_pads(blink).await;
                draw_all_clears(blink).await;
                draw_all_adds(blink).await;
            }
            _ = clean_tick.tick() => {
                engine().cleanup()
            }
        }
    }
}
