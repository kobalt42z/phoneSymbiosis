//! TODO: implement `phonesym sms ...` commands.

use crate::Ctx;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum SmsCmd {
    /// Placeholder until implemented.
    List,
}

pub async fn run(_ctx: &Ctx, _cmd: SmsCmd) -> anyhow::Result<()> {
    anyhow::bail!("not implemented yet")
}
