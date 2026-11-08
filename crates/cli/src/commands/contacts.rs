//! `phonesym contacts ...`: read the phone's address book and call log (PBAP).
//!
//! Everything is downloaded fresh from the phone on each run and nothing is
//! stored on this PC. On an iPhone, enable "Sync Contacts" for this PC in
//! Settings > Bluetooth > (i).

use crate::commands::pick_phone;
use crate::Ctx;
use clap::{Subcommand, ValueEnum};
use phonesym_platform::{CallHistoryEntry, CallHistoryKind, Contact};

#[derive(Subcommand)]
pub enum ContactsCmd {
    /// List contacts.
    List {
        /// Phone address (defaults to the first connected phone).
        #[arg(long)]
        phone: Option<String>,
        /// Only show contacts whose name, number, e-mail or company contains this text.
        #[arg(long)]
        search: Option<String>,
        /// Include contact photos (base64) in JSON output.
        #[arg(long)]
        photos: bool,
    },
    /// Show the call history.
    History {
        /// Phone address (defaults to the first connected phone).
        #[arg(long)]
        phone: Option<String>,
        /// Only this kind of call (default: all).
        #[arg(long, value_enum)]
        kind: Option<KindArg>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum KindArg {
    Incoming,
    Outgoing,
    Missed,
}

impl From<KindArg> for CallHistoryKind {
    fn from(k: KindArg) -> Self {
        match k {
            KindArg::Incoming => CallHistoryKind::Incoming,
            KindArg::Outgoing => CallHistoryKind::Outgoing,
            KindArg::Missed => CallHistoryKind::Missed,
        }
    }
}

pub async fn run(ctx: &Ctx, cmd: ContactsCmd) -> anyhow::Result<()> {
    match cmd {
        ContactsCmd::List {
            phone,
            search,
            photos,
        } => {
            let phone = pick_phone(&ctx.platform, phone.as_deref()).await?;
            eprintln!("Downloading contacts from {phone}...");
            let mut contacts = ctx.platform.contacts.contacts(&phone).await?;
            if let Some(q) = search.as_deref() {
                contacts.retain(|c| matches(c, q));
            }
            if !photos {
                for c in &mut contacts {
                    c.photo_base64 = None;
                }
            }
            contacts.sort_by_key(|c| c.name.to_lowercase());
            ctx.print(&contacts, |cs| print_contacts(cs));
        }
        ContactsCmd::History { phone, kind } => {
            let phone = pick_phone(&ctx.platform, phone.as_deref()).await?;
            eprintln!("Downloading call history from {phone}...");
            let entries = ctx
                .platform
                .contacts
                .call_history(&phone, kind.map(Into::into))
                .await?;
            ctx.print(&entries, |es| print_history(es));
        }
    }
    Ok(())
}

/// Case-insensitive match on text fields; numbers also match ignoring
/// spaces and dashes, so "5550100" finds "+1 555-0100".
fn matches(c: &Contact, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    let text_hit = c.name.to_lowercase().contains(&q)
        || c.emails.iter().any(|e| e.to_lowercase().contains(&q))
        || c.organization
            .as_deref()
            .is_some_and(|o| o.to_lowercase().contains(&q))
        || c.numbers.iter().any(|n| n.number.contains(&q));
    if text_hit {
        return true;
    }
    // Query that looks like a number: compare digits only.
    let looks_like_number = q
        .chars()
        .all(|ch| ch.is_ascii_digit() || " -()+.".contains(ch));
    let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
    let q_digits = digits(&q);
    looks_like_number
        && !q_digits.is_empty()
        && c.numbers
            .iter()
            .any(|n| digits(&n.number).contains(&q_digits))
}

fn print_contacts(contacts: &[Contact]) {
    if contacts.is_empty() {
        println!("No contacts.");
        return;
    }
    for c in contacts {
        match &c.organization {
            Some(org) if *org != c.name => println!("{}  ({org})", c.name),
            _ => println!("{}", c.name),
        }
        for n in &c.numbers {
            println!(
                "    {:<8} {}",
                n.kind.as_deref().unwrap_or("phone"),
                n.number
            );
        }
        for e in &c.emails {
            println!("    {:<8} {e}", "email");
        }
    }
    println!("\n{} contact(s)", contacts.len());
}

fn print_history(entries: &[CallHistoryEntry]) {
    if entries.is_empty() {
        println!("No calls.");
        return;
    }
    for e in entries {
        let kind = match e.kind {
            CallHistoryKind::Incoming => "↙ in ",
            CallHistoryKind::Outgoing => "↗ out",
            CallHistoryKind::Missed => "✗ missed",
        };
        let when = e
            .timestamp
            .as_deref()
            .map(format_timestamp)
            .unwrap_or_default();
        let who = match (&e.name, e.number.is_empty()) {
            (Some(name), false) => format!("{name} ({})", e.number),
            (Some(name), true) => name.clone(),
            (None, false) => e.number.clone(),
            (None, true) => "unknown".into(),
        };
        println!("{when:<16}  {kind:<9} {who}");
    }
}

/// `20261108T110455[Z|+0100]` -> `2026-11-08 11:04`; anything else is shown as is.
pub fn format_timestamp(ts: &str) -> String {
    let b = ts.as_bytes();
    if b.len() >= 13
        && b[8] == b'T'
        && ts.is_char_boundary(13)
        && b[..8].iter().all(u8::is_ascii_digit)
    {
        format!(
            "{}-{}-{} {}:{}",
            &ts[0..4],
            &ts[4..6],
            &ts[6..8],
            &ts[9..11],
            &ts[11..13]
        )
    } else {
        ts.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use phonesym_platform::PhoneNumber;

    fn contact() -> Contact {
        Contact {
            id: "1".into(),
            name: "Anna Müller".into(),
            numbers: vec![PhoneNumber {
                number: "+1 555-0100".into(),
                kind: Some("cell".into()),
            }],
            emails: vec!["anna@example.com".into()],
            organization: Some("Acme".into()),
            photo_base64: None,
        }
    }

    #[test]
    fn search() {
        let c = contact();
        assert!(matches(&c, "müll"));
        assert!(matches(&c, "ACME"));
        assert!(matches(&c, "example"));
        assert!(matches(&c, "5550100"));
        assert!(matches(&c, "555 0100"));
        assert!(!matches(&c, "bob"));
        assert!(!matches(&c, "999"));
    }

    #[test]
    fn timestamps() {
        assert_eq!(format_timestamp("20261108T110455"), "2026-11-08 11:04");
        assert_eq!(format_timestamp("20261108T110455Z"), "2026-11-08 11:04");
        assert_eq!(format_timestamp("yesterday"), "yesterday");
    }
}
