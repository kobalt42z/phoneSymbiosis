//! `phonesym sms ...`: list, read, send and watch SMS (MAP).
//!
//! On an iPhone, enable "Show Notifications" for this PC in
//! Settings > Bluetooth > (i). iMessages are not available over Bluetooth.

use crate::commands::contacts::format_timestamp;
use crate::commands::pick_phone;
use crate::Ctx;
use clap::{Subcommand, ValueEnum};
use phonesym_platform::{Message, MessageFolder, PhoneEvent};
use tokio::sync::broadcast::error::RecvError;

#[derive(Subcommand)]
pub enum SmsCmd {
    /// List messages in a folder, newest first.
    List {
        /// Phone address (defaults to the first connected phone).
        #[arg(long)]
        phone: Option<String>,
        /// Folder to list.
        #[arg(long, value_enum, default_value_t = FolderArg::Inbox)]
        folder: FolderArg,
        /// Maximum number of messages.
        #[arg(long, default_value_t = 20)]
        limit: u16,
    },
    /// Show the full text of one message (ID from `sms list`).
    Read {
        id: String,
        /// Phone address (defaults to the first connected phone).
        #[arg(long)]
        phone: Option<String>,
    },
    /// Send an SMS through the phone.
    Send {
        /// Recipient phone number.
        number: String,
        /// Message text (several words are joined with spaces).
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
        /// Phone address (defaults to the first connected phone).
        #[arg(long)]
        phone: Option<String>,
    },
    /// Print new incoming messages until Ctrl+C.
    Watch {
        /// Phone address (defaults to the first connected phone).
        #[arg(long)]
        phone: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum FolderArg {
    Inbox,
    Sent,
    Outbox,
    Draft,
    Deleted,
}

impl From<FolderArg> for MessageFolder {
    fn from(f: FolderArg) -> Self {
        match f {
            FolderArg::Inbox => MessageFolder::Inbox,
            FolderArg::Sent => MessageFolder::Sent,
            FolderArg::Outbox => MessageFolder::Outbox,
            FolderArg::Draft => MessageFolder::Draft,
            FolderArg::Deleted => MessageFolder::Deleted,
        }
    }
}

pub async fn run(ctx: &Ctx, cmd: SmsCmd) -> anyhow::Result<()> {
    let messages = &ctx.platform.messages;
    match cmd {
        SmsCmd::List {
            phone,
            folder,
            limit,
        } => {
            let phone = pick_phone(&ctx.platform, phone.as_deref()).await?;
            let list = messages.list(&phone, folder.into(), limit).await?;
            ctx.print(&list, |l| print_list(l));
        }
        SmsCmd::Read { id, phone } => {
            let phone = pick_phone(&ctx.platform, phone.as_deref()).await?;
            let message = messages.get(&phone, &id).await?;
            ctx.print(&message, print_full);
        }
        SmsCmd::Send {
            number,
            text,
            phone,
        } => {
            let phone = pick_phone(&ctx.platform, phone.as_deref()).await?;
            let text = text.join(" ");
            messages.send(&phone, &number, &text).await?;
            ctx.print(&serde_json::json!({ "sent": true, "to": number }), |_| {
                println!("Sent to {number}.")
            });
        }
        SmsCmd::Watch { phone } => {
            let phone = pick_phone(&ctx.platform, phone.as_deref()).await?;
            // Subscribe first so nothing is missed between starting and listening.
            let mut rx = ctx.platform.events.subscribe();
            messages.watch(&phone).await?;
            eprintln!("Watching for new SMS on {phone}, press Ctrl+C to stop.");
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => break,
                    ev = rx.recv() => match ev {
                        Ok(PhoneEvent::MessageReceived { message }) => {
                            ctx.print(&message, |m| println!("💬 {}", one_line(m)));
                        }
                        Ok(PhoneEvent::Warning { message }) => eprintln!("⚠️  {message}"),
                        Ok(_) => {}
                        Err(RecvError::Lagged(n)) => eprintln!("(missed {n} events)"),
                        Err(RecvError::Closed) => break,
                    }
                }
            }
        }
    }
    Ok(())
}

fn peer(m: &Message) -> String {
    match &m.peer_name {
        Some(name) if !m.peer_number.is_empty() => format!("{name} ({})", m.peer_number),
        Some(name) => name.clone(),
        None if m.peer_number.is_empty() => "unknown".into(),
        None => m.peer_number.clone(),
    }
}

fn one_line(m: &Message) -> String {
    let when = m
        .timestamp
        .as_deref()
        .map(format_timestamp)
        .unwrap_or_default();
    let text: String = m.text.replace(['\r', '\n'], " ");
    let preview = if text.chars().count() > 80 {
        format!("{}…", text.chars().take(79).collect::<String>())
    } else {
        text
    };
    format!("{when:<16} {}: {preview}", peer(m))
}

fn print_list(list: &[Message]) {
    if list.is_empty() {
        println!("No messages.");
        return;
    }
    for m in list {
        let unread = if m.read { ' ' } else { '•' };
        println!("{unread} [{}] {}", m.id, one_line(m));
    }
    println!("\nRead one with: phonesym sms read <ID>");
}

fn print_full(m: &Message) {
    let direction = match m.folder {
        MessageFolder::Sent | MessageFolder::Outbox | MessageFolder::Draft => "To",
        _ => "From",
    };
    println!("{direction}: {}", peer(m));
    if let Some(ts) = &m.timestamp {
        println!("Date: {}", format_timestamp(ts));
    }
    println!("Folder: {}", m.folder.map_name());
    println!();
    println!("{}", m.text);
}
