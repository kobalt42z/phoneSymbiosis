//! TODO: implement `phonesym device ...` commands.

use crate::Ctx;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum DeviceCmd {
    /// Placeholder until implemented.
    List,
}

pub async fn run(_ctx: &Ctx, _cmd: DeviceCmd) -> anyhow::Result<()> {
    anyhow::bail!("not implemented yet")
}
