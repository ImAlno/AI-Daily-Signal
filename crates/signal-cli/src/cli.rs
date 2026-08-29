#[derive(clap::Parser)]
#[command(name = "signal", version, about = "A calm daily AI briefing")]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true, conflicts_with = "json")]
    pub plain: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(clap::Subcommand)]
pub enum Command {
    Init,
    Refresh,
    Today {
        #[arg(long)]
        refresh: bool,
    },
    Latest {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Show {
        id: String,
    },
    Save {
        id: String,
        #[arg(long)]
        remove: bool,
    },
    Saved,
    Status,
    Sources {
        #[command(subcommand)]
        command: SourceCommand,
    },
}

#[derive(clap::Subcommand)]
pub enum SourceCommand {
    List,
    Enable { id: String },
    Disable { id: String },
}
