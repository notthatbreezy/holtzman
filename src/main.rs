use std::{path::PathBuf, sync::Arc};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use holtzman::{
    automation::Pilot,
    bindings::Bindings,
    config::Config,
    input::platform_sender,
    journal::JournalDir,
    screen::{Format, platform_screen, render},
    server,
};

/// Ship computer for Elite Dangerous, served over MCP.
#[derive(Parser)]
#[command(version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("HOLTZMAN_GIT_COMMIT"), ")"), about)]
struct Cli {
    /// Path to holtzman.toml
    #[arg(long, short, global = true, default_value = "holtzman.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write a starter holtzman.toml with a fresh token.
    Init {
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Run the MCP server.
    Serve,
    /// Print the current status snapshot (sanity-check the journal folder).
    Status,
    /// List the keyboard bindings holtzman found.
    Bindings,
    /// Replace the plot_route recipe in holtzman.toml with the current default
    /// (keeps your token, hosts and recorded click targets).
    ResetRecipe,
    /// Print the mouse position relative to the game window (for click targets).
    Cursor,
    /// Save a screenshot of the game window (checks capture works).
    Screenshot {
        /// Output file; .png or .jpg
        #[arg(default_value = "holtzman-screenshot.png")]
        out: PathBuf,
        /// Named region (full, center, left, right, top, bottom, or [screen_regions])
        #[arg(long, default_value = "full")]
        region: String,
        /// Longest side in pixels; 0 keeps full resolution
        #[arg(long, default_value_t = 0)]
        max_edge: u32,
    },
    /// Run the plot-route recipe once, without MCP (for tuning).
    Plot {
        system: String,
        /// Log keys instead of sending them.
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,rmcp=warn".into()),
        )
        .init();
    let cli = Cli::parse();

    match cli.command {
        Command::Init { force } => {
            if cli.config.exists() && !force {
                bail!("{} exists; pass --force to overwrite", cli.config.display());
            }
            let config = Config::generate();
            std::fs::write(&cli.config, config.to_toml()?)?;
            println!("wrote {}", cli.config.display());
            println!("token: {}", config.token);
            println!("add this PC's LAN IP to allowed_hosts if clients connect by IP");
        }
        Command::Serve => server::serve(Config::load(&cli.config)?, cli.config.clone()).await?,
        Command::Status => {
            let config = Config::load(&cli.config)?;
            let dir = config
                .journal_dir()
                .ok_or_else(|| anyhow::anyhow!("no journal_dir"))?;
            let summary = JournalDir::new(dir).summary()?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Command::Bindings => {
            let config = Config::load(&cli.config)?;
            let dir = config
                .bindings_dir()
                .ok_or_else(|| anyhow::anyhow!("no bindings_dir"))?;
            let b = Bindings::discover(&dir)?;
            if let Some(src) = &b.source {
                println!("# {}", src.display());
            }
            for (action, chord) in &b.keyboard {
                println!("{action:40} {chord}");
            }
            for action in holtzman::actions::coverage(&config, &b) {
                println!(
                    "{} {} {}",
                    action["status"], action["action"], action["binding"]
                );
            }
            for needed in ["GalaxyMapOpen", "UI_Up", "UI_Select"] {
                if !b.keyboard.contains_key(needed) {
                    println!("!! {needed} has no keyboard binding; plot_route needs one");
                }
            }
        }
        Command::ResetRecipe => {
            let mut config = Config::load(&cli.config)?;
            config.plot_route = holtzman::config::PlotRoute::default();
            config.save(&cli.config)?;
            println!("plot_route recipe reset in {}", cli.config.display());
            let missing = config.missing_click_targets();
            if !missing.is_empty() {
                println!(
                    "still needed: {missing:?}. Hover over the button and run the \
                     record_click_target tool (or `holtzman cursor` to see coordinates)"
                );
            }
        }
        Command::Cursor => {
            let sender = platform_sender(false);
            let pos = sender.cursor()?;
            println!("{}", serde_json::to_string_pretty(&pos)?);
        }
        Command::Screenshot {
            out,
            region,
            max_edge,
        } => {
            let config = Config::load(&cli.config)?;
            let region = config.screen_region(&region)?;
            let format = match out.extension().and_then(|e| e.to_str()) {
                Some("jpg" | "jpeg") => Format::Jpeg,
                _ => Format::Png,
            };
            let frame = platform_screen().capture()?;
            let max_edge = if max_edge == 0 { u32::MAX } else { max_edge };
            let shot = render(
                &frame,
                region,
                max_edge,
                format,
                config.screenshot.jpeg_quality,
            )?;
            std::fs::write(&out, &shot.bytes)?;
            println!(
                "wrote {} ({}x{} from a {}x{} window)",
                out.display(),
                shot.width,
                shot.height,
                shot.window_width,
                shot.window_height
            );
        }
        Command::Plot { system, dry_run } => {
            let mut config = Config::load(&cli.config)?;
            config.dry_run |= dry_run;
            let journal = JournalDir::new(
                config
                    .journal_dir()
                    .ok_or_else(|| anyhow::anyhow!("no journal_dir"))?,
            );
            let bindings = Bindings::discover(
                &config
                    .bindings_dir()
                    .ok_or_else(|| anyhow::anyhow!("no bindings_dir"))?,
            )?;
            let sender: Arc<_> = platform_sender(config.dry_run);
            println!("switch to the game: starting in 3 s");
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let pilot = Pilot {
                config: &config,
                bindings: &bindings,
                journal: &journal,
                sender: sender.as_ref(),
            };
            println!("{}", pilot.plot_route(&system).await?);
        }
    }
    Ok(())
}
