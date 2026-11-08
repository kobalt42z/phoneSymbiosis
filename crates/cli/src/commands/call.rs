//! TODO: implement `phonesym call ...` commands.

use crate::Ctx;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum CallCmd {
    /// Placeholder until implemented.
    List,
}

pub async fn run(_ctx: &Ctx, _cmd: CallCmd) -> anyhow::Result<()> {
    anyhow::bail!("not implemented yet")
}
