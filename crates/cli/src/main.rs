//! `phonesym`: try each feature from the terminal.
//!
//! Each feature area has its own file in `commands/`.

mod commands;

use clap::{Parser, Subcommand};
use phonesym_platform::PlatformConfig;

#[derive(Parser)]
#[command(name = "phonesym", version, about = "Use your phone's calls, contacts and SMS from this PC")]
struct Cli {
    /// Bluetooth adapter to use (e.g. hci0). Defaults to the first one.
    #[arg(long, global = true)]
    adapter: Option<String>,

    /// Print results as JSON.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check that the required system services are present.
    Doctor,
    /// Bluetooth: scan, pair, connect.
    #[command(subcommand)]
    Device(commands::device::DeviceCmd),
    /// Calls: dial, answer, hang up.
    #[command(subcommand)]
    Call(commands::call::CallCmd),
    /// Contacts and call history.
    #[command(subcommand)]
    Contacts(commands::contacts::ContactsCmd),
    /// SMS messages.
    #[command(subcommand)]
    Sms(commands::sms::SmsCmd),
    /// Print live events (incoming calls, SMS, connections) until Ctrl+C.
    Watch {
        /// Also route call audio automatically when a call is active.
        #[arg(long)]
        audio: bool,
    },
}

pub struct Ctx {
    pub platform: phonesym_platform::Platform,
    pub json: bool,
}

impl Ctx {
    /// Print a value as JSON or with the given human formatter.
    pub fn print<T: serde::Serialize>(&self, value: &T, human: impl FnOnce(&T)) {
        if self.json {
            println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
        } else {
            human(value);
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let auto_audio = matches!(cli.command, Command::Watch { audio: true });
    let config = PlatformConfig { adapter: cli.adapter.clone(), auto_audio };

    if let Command::Doctor = cli.command {
        return commands::doctor::run(config).await;
    }

    let platform = phonesym_platform::init(config).await?;
    let ctx = Ctx { platform, json: cli.json };

    match cli.command {
        Command::Doctor => unreachable!(),
        Command::Device(c) => commands::device::run(&ctx, c).await,
        Command::Call(c) => commands::call::run(&ctx, c).await,
        Command::Contacts(c) => commands::contacts::run(&ctx, c).await,
        Command::Sms(c) => commands::sms::run(&ctx, c).await,
        Command::Watch { .. } => commands::watch::run(&ctx).await,
    }
}
