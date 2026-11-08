//! `phonesym call ...`: make and control phone calls (Hands-Free Profile).
//!
//! The phone must be connected (`phonesym device connect`). Call audio goes
//! to this PC only while `phonesym call audio on` (or `phonesym watch --audio`)
//! is running.

use crate::commands::{print_done, OnOff};
use crate::Ctx;
use clap::Subcommand;
use phonesym_platform::{Address, Call, CallDirection, CallId, CallState, CallVolume, PhoneStatus};

#[derive(Subcommand)]
pub enum CallCmd {
    /// List current calls.
    List,
    /// Call a number.
    Dial {
        number: String,
        /// Phone address (defaults to the first connected phone).
        #[arg(long)]
        phone: Option<String>,
    },
    /// Answer a call (default: the ringing one).
    Answer { call_id: Option<String> },
    /// Hang up one call, or all calls when no id is given.
    Hangup {
        call_id: Option<String>,
        #[arg(long)]
        phone: Option<String>,
    },
    /// Send keypad tones during a call (e.g. for "press 1 for...").
    Dtmf {
        digits: String,
        #[arg(long)]
        phone: Option<String>,
    },
    /// Swap the active and held calls.
    Swap {
        #[arg(long)]
        phone: Option<String>,
    },
    /// Hold the current call and answer the waiting one.
    HoldAnswer {
        #[arg(long)]
        phone: Option<String>,
    },
    /// End the current call and answer the waiting one.
    ReleaseAnswer {
        #[arg(long)]
        phone: Option<String>,
    },
    /// Join the active and held calls into a conference.
    Merge {
        #[arg(long)]
        phone: Option<String>,
    },
    /// Show or set call volume (0-15).
    Volume {
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=15))]
        speaker: Option<u8>,
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=15))]
        mic: Option<u8>,
        #[arg(long)]
        phone: Option<String>,
    },
    /// Show the phone's status.
    Status {
        #[arg(long)]
        phone: Option<String>,
    },
    /// Route call audio to this PC's speakers and microphone.
    /// `on` keeps running until Ctrl+C.
    Audio {
        #[arg(value_enum)]
        state: OnOff,
        #[arg(long)]
        phone: Option<String>,
    },
}

fn parse_phone(phone: Option<String>) -> anyhow::Result<Option<Address>> {
    Ok(phone.as_deref().map(Address::parse).transpose()?)
}

pub async fn run(ctx: &Ctx, cmd: CallCmd) -> anyhow::Result<()> {
    let calls = &ctx.platform.calls;
    match cmd {
        CallCmd::List => {
            let list = calls.calls().await?;
            ctx.print(&list, |l| print_calls(l));
        }
        CallCmd::Dial { number, phone } => {
            let phone = parse_phone(phone)?;
            calls.dial(phone.as_ref(), &number).await?;
            print_done(ctx, &format!("Calling {number}..."));
        }
        CallCmd::Answer { call_id } => {
            let id = match call_id {
                Some(id) => CallId(id),
                None => ringing_call(&calls.calls().await?)?,
            };
            calls.answer(&id).await?;
            print_done(ctx, "Answered.");
        }
        CallCmd::Hangup {
            call_id: Some(id), ..
        } => {
            calls.hang_up(&CallId(id)).await?;
            print_done(ctx, "Hung up.");
        }
        CallCmd::Hangup {
            call_id: None,
            phone,
        } => {
            calls.hang_up_all(parse_phone(phone)?.as_ref()).await?;
            print_done(ctx, "Hung up all calls.");
        }
        CallCmd::Dtmf { digits, phone } => {
            calls
                .send_tones(parse_phone(phone)?.as_ref(), &digits)
                .await?;
            print_done(ctx, &format!("Sent {digits}."));
        }
        CallCmd::Swap { phone } => {
            calls.swap_calls(parse_phone(phone)?.as_ref()).await?;
            print_done(ctx, "Swapped calls.");
        }
        CallCmd::HoldAnswer { phone } => {
            calls.hold_and_answer(parse_phone(phone)?.as_ref()).await?;
            print_done(ctx, "Holding the current call, answered the waiting one.");
        }
        CallCmd::ReleaseAnswer { phone } => {
            calls
                .release_and_answer(parse_phone(phone)?.as_ref())
                .await?;
            print_done(ctx, "Ended the current call, answered the waiting one.");
        }
        CallCmd::Merge { phone } => {
            calls.merge_calls(parse_phone(phone)?.as_ref()).await?;
            print_done(ctx, "Calls merged.");
        }
        CallCmd::Volume {
            speaker,
            mic,
            phone,
        } => volume(ctx, parse_phone(phone)?, speaker, mic).await?,
        CallCmd::Status { phone } => {
            let status = calls.status(parse_phone(phone)?.as_ref()).await?;
            ctx.print(&status, print_status);
        }
        CallCmd::Audio {
            state: OnOff::On,
            phone,
        } => audio_on(ctx, parse_phone(phone)?).await?,
        CallCmd::Audio {
            state: OnOff::Off, ..
        } => {
            ctx.platform.audio.stop().await?;
            print_done(ctx, "Call audio routing stopped.");
        }
    }
    Ok(())
}

/// The call to answer when the user didn't say which.
fn ringing_call(calls: &[Call]) -> anyhow::Result<CallId> {
    if let Some(c) = calls.iter().find(|c| c.state == CallState::Incoming) {
        return Ok(c.id.clone());
    }
    if calls.iter().any(|c| c.state == CallState::Waiting) {
        anyhow::bail!("the ringing call is waiting behind another call: use `call hold-answer` or `call release-answer`");
    }
    anyhow::bail!("no incoming call to answer")
}

async fn volume(
    ctx: &Ctx,
    phone: Option<Address>,
    speaker: Option<u8>,
    mic: Option<u8>,
) -> anyhow::Result<()> {
    let calls = &ctx.platform.calls;
    let mut v = calls.volume(phone.as_ref()).await?;
    if speaker.is_some() || mic.is_some() {
        v = CallVolume {
            speaker: speaker.unwrap_or(v.speaker),
            microphone: mic.unwrap_or(v.microphone),
        };
        calls.set_volume(phone.as_ref(), v).await?;
    }
    ctx.print(&v, |v| {
        println!("speaker {}/15, microphone {}/15", v.speaker, v.microphone)
    });
    Ok(())
}

/// Route audio until Ctrl+C, then undo it.
async fn audio_on(ctx: &Ctx, phone: Option<Address>) -> anyhow::Result<()> {
    let phone = match phone {
        Some(p) => p,
        None => ctx
            .platform
            .calls
            .available_phones()
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no phone with a hands-free link; connect one first"))?,
    };
    ctx.platform.audio.start(&phone).await?;
    print_done(
        ctx,
        &format!("Call audio from {phone} is on this PC. Press Ctrl+C to stop."),
    );
    tokio::signal::ctrl_c().await?;
    ctx.platform.audio.stop().await?;
    Ok(())
}

fn print_calls(calls: &[Call]) {
    if calls.is_empty() {
        println!("No calls.");
        return;
    }
    for c in calls {
        let arrow = match c.direction {
            CallDirection::Incoming => "←",
            CallDirection::Outgoing => "→",
        };
        let who = match &c.name {
            Some(n) => format!("{n} ({})", c.number),
            None if c.number.is_empty() => "unknown".to_owned(),
            None => c.number.clone(),
        };
        let conf = if c.multiparty { " [conference]" } else { "" };
        println!(
            "{arrow} {who:<30} {:<12}{conf}  id={}",
            format!("{:?}", c.state).to_lowercase(),
            c.id
        );
    }
}

fn print_status(s: &PhoneStatus) {
    let show = |v: Option<String>| v.unwrap_or_else(|| "not reported".into());
    println!(
        "phone:    {}",
        show(s.device.as_ref().map(ToString::to_string))
    );
    println!("battery:  {}", show(s.battery.map(|b| format!("{b}%"))));
    println!("signal:   {}", show(s.signal.map(|b| format!("{b}/5"))));
    println!("operator: {}", show(s.operator.clone()));
    if let Some(r) = s.roaming {
        println!("roaming:  {}", if r { "yes" } else { "no" });
    }
}
