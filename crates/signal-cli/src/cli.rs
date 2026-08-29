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
    Refresh {
        #[arg(long)]
        no_ai: bool,
    },
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
    Models {
        #[command(subcommand)]
        command: ModelCommand,
    },
    Summarize {
        story_id: String,
        #[arg(long, alias = "model")]
        profile: Option<String>,
        #[arg(long)]
        force: bool,
    },
}

#[derive(clap::Subcommand)]
pub enum SourceCommand {
    List,
    Enable { id: String },
    Disable { id: String },
}

#[derive(clap::Subcommand)]
pub enum ModelCommand {
    List,
    Add(ModelAddArgs),
    Use {
        profile: String,
    },
    Test {
        profile: String,
    },
    Remove {
        profile: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(clap::Args)]
pub struct ModelAddArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub provider: ProviderArg,
    #[arg(long)]
    pub model: String,
    #[arg(long)]
    pub endpoint: Option<String>,
    #[arg(long)]
    pub dialect: Option<DialectArg>,
    #[arg(long)]
    pub credential_env: Option<String>,
    #[arg(long)]
    pub max_summaries: Option<u32>,
    #[arg(long)]
    pub daily_budget_usd: Option<String>,
    #[arg(long)]
    pub input_usd_per_million: Option<String>,
    #[arg(long)]
    pub output_usd_per_million: Option<String>,
    #[arg(long)]
    pub max_output_tokens: Option<u32>,
    #[arg(long)]
    pub timeout_seconds: Option<u64>,
    #[arg(long)]
    pub max_retries: Option<u32>,
    #[arg(long)]
    pub consent_provider_data_sharing: bool,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum ProviderArg {
    #[value(name = "open-ai")]
    OpenAi,
    Anthropic,
    Gemini,
    #[value(name = "open-ai-compatible")]
    OpenAiCompatible,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum DialectArg {
    Responses,
    #[value(name = "chat-completions")]
    ChatCompletions,
}
