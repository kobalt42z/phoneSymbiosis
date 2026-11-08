//! TODO: implement `phonesym contacts ...` commands.

use crate::Ctx;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ContactsCmd {
    /// Placeholder until implemented.
    List,
}

pub async fn run(_ctx: &Ctx, _cmd: ContactsCmd) -> anyhow::Result<()> {
    anyhow::bail!("not implemented yet")
}
