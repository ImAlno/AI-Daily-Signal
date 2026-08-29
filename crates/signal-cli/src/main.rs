mod cli;
mod output;

use chrono::Utc;
use clap::Parser;
use cli::{Cli, Command, SourceCommand};
use signal_core::{SignalApp, SignalError, TodayView};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli).await {
        eprintln!("{}", display_error(&error));
        std::process::exit(exit_code(&error));
    }
}

async fn run(cli: Cli) -> signal_core::Result<()> {
    let Cli {
        json,
        plain: _,
        command,
    } = cli;
    let mut app = SignalApp::open()?;
    match command {
        Command::Init => {
            let status = app.init()?;
            print_value(json, output::status(&status), &status)?;
        }
        Command::Refresh => {
            let report = app.refresh(Utc::now()).await?;
            print_value(json, output::refresh(&report), &report)?;
        }
        Command::Today { refresh } => {
            let now = Utc::now();
            let view = if refresh {
                TodayView::fresh(app.refresh(now).await?.briefing)
            } else {
                app.today(now)?
            };
            print_value(json, output::today(&view), &view)?;
        }
        Command::Latest { limit } => {
            let stories = app.latest(limit)?;
            print_value(json, output::stories(&stories), &stories)?;
        }
        Command::Show { id } => {
            let story = app.show(&id)?;
            print_value(json, output::story(&story), &story)?;
        }
        Command::Save { id, remove } => {
            let story = app.set_saved(&id, !remove)?;
            print_value(json, output::story(&story), &story)?;
        }
        Command::Saved => {
            let stories = app.saved()?;
            print_value(json, output::stories(&stories), &stories)?;
        }
        Command::Status => {
            let status = app.status()?;
            print_value(json, output::status(&status), &status)?;
        }
        Command::Sources { command } => match command {
            SourceCommand::List => {
                let sources = app.list_sources();
                print_value(json, output::sources(&sources), &sources)?;
            }
            SourceCommand::Enable { id } => {
                let source = app.set_source_enabled(&id, true)?;
                print_value(
                    json,
                    output::sources(std::slice::from_ref(&source)),
                    &source,
                )?;
            }
            SourceCommand::Disable { id } => {
                let source = app.set_source_enabled(&id, false)?;
                print_value(
                    json,
                    output::sources(std::slice::from_ref(&source)),
                    &source,
                )?;
            }
        },
    }
    Ok(())
}

fn print_value<T: serde::Serialize>(
    json: bool,
    human: String,
    value: &T,
) -> signal_core::Result<()> {
    if json {
        println!("{}", output::json(value)?);
    } else {
        println!("{human}");
    }
    Ok(())
}

fn exit_code(error: &SignalError) -> i32 {
    match error {
        SignalError::InvalidConfiguration(_)
        | SignalError::Io(_)
        | SignalError::Serialization(_) => 2,
        SignalError::Network(_) | SignalError::Feed(_) | SignalError::Refresh(_) => 3,
        SignalError::NotFound(_) => 4,
        SignalError::Database(_) | SignalError::Storage(_) | SignalError::Credential(_) => 5,
    }
}

fn display_error(error: &SignalError) -> String {
    match error {
        SignalError::InvalidConfiguration(message) => format!("Configuration error: {message}"),
        SignalError::Io(_) | SignalError::Serialization(_) => {
            "Configuration could not be read or written".to_owned()
        }
        SignalError::Network(_) | SignalError::Feed(_) | SignalError::Refresh(_) => {
            "Refresh failed".to_owned()
        }
        SignalError::NotFound(message) => message.clone(),
        SignalError::Database(_) | SignalError::Storage(_) => "Storage operation failed".to_owned(),
        SignalError::Credential(_) => "Credential operation failed".to_owned(),
    }
}
