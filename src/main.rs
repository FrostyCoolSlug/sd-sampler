use openaction::OpenActionResult;

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

    println!("Hello, world!");

    Ok(())
}
