mod cli;
mod output;

use std::io::{IsTerminal, Write};

use chrono::Utc;
use clap::Parser;
use cli::{Cli, Command, DialectArg, ModelAddArgs, ModelCommand, ProviderArg, SourceCommand};
use signal_core::{
    AddModelCredential, AddModelInput, ApiDialect, CredentialRef, ManualGenerationStatus,
    MoneyMicros, NewModelProfile, ProfileLimits, ProviderKind, RefreshOptions, SignalApp,
    SignalError, SummarizeOptions, SummaryVariant, TestModelOptions, TodayView,
};

#[derive(Debug)]
enum CliError {
    Core(SignalError),
    ExplicitGeneration(ManualGenerationStatus),
}

impl From<SignalError> for CliError {
    fn from(error: SignalError) -> Self {
        Self::Core(error)
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(cli).await {
        eprintln!("{}", display_error(&error));
        std::process::exit(exit_code(&error));
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
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
        Command::Refresh { no_ai } => {
            let report = app
                .refresh_with_options(Utc::now(), RefreshOptions { ai: !no_ai })
                .await?;
            let safe = output::refresh_data(&report);
            print_value(json, output::refresh(&safe), &safe)?;
        }
        Command::Today { refresh } => {
            let now = Utc::now();
            let view = if refresh {
                TodayView::fresh(app.refresh(now).await?.briefing)
            } else {
                app.today(now)?
            };
            let safe = output::today_data(&view);
            print_value(json, output::today(&safe), &safe)?;
        }
        Command::Latest { limit } => {
            let stories = app.latest(limit)?;
            print_value(json, output::stories(&stories), &stories)?;
        }
        Command::Show { id } => {
            let story = app.show(&id)?;
            let selected = selected_summary_for_story(&app, &story.id)?;
            let safe = output::story_data(&story, selected.as_ref());
            print_value(json, output::story(&safe), &safe)?;
        }
        Command::Save { id, remove } => {
            let story = app.set_saved(&id, !remove)?;
            let selected = selected_summary_for_story(&app, &story.id)?;
            let safe = output::story_data(&story, selected.as_ref());
            print_value(json, output::story(&safe), &safe)?;
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
        Command::Models { command } => match command {
            ModelCommand::List => {
                let profiles = app.list_models()?;
                let default_id = app.default_model_profile()?.map(|profile| profile.id);
                let safe = output::model_profiles_data(&profiles, default_id);
                print_value(json, output::model_profiles(&safe), &safe)?;
            }
            ModelCommand::Add(args) => {
                let report = app.add_model(model_add_input(args)?, Utc::now())?;
                let safe = output::model_profile_data(&report.profile, false);
                print_value(json, output::model_profile(&safe), &safe)?;
            }
            ModelCommand::Use { profile } => {
                let profile = app.use_model(&profile)?;
                let safe = output::model_profile_data(&profile, true);
                print_value(json, output::model_profile(&safe), &safe)?;
            }
            ModelCommand::Test { profile } => {
                eprintln!("Warning: this model test may incur provider cost.");
                let report = app
                    .test_model(TestModelOptions { profile }, Utc::now())
                    .await?;
                let status = report.status;
                let safe = output::test_model_data(&report);
                print_value(json, output::test_model(&safe), &safe)?;
                ensure_explicit_generation_succeeded(status)?;
            }
            ModelCommand::Remove { profile, yes } => {
                confirm_model_removal(yes)?;
                let report = app.remove_model(&profile)?;
                print_value(json, output::remove_model(&report), &report)?;
            }
        },
        Command::Summarize {
            story_id,
            profile,
            force,
        } => {
            let report = app
                .summarize_story(&story_id, SummarizeOptions { profile, force }, Utc::now())
                .await?;
            let status = report.status;
            let safe = output::summarize_data(&report);
            print_value(json, output::summarize(&safe), &safe)?;
            ensure_explicit_generation_succeeded(status)?;
        }
    }
    Ok(())
}

fn selected_summary_for_story(
    app: &SignalApp,
    story_id: &str,
) -> signal_core::Result<Option<SummaryVariant>> {
    match app.today(Utc::now()) {
        Ok(view) => Ok(view
            .briefing
            .items
            .into_iter()
            .find(|item| item.story.id == story_id)
            .and_then(|item| item.selected_summary)),
        Err(SignalError::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn ensure_explicit_generation_succeeded(status: ManualGenerationStatus) -> Result<(), CliError> {
    if status.is_success() {
        Ok(())
    } else {
        Err(CliError::ExplicitGeneration(status))
    }
}

fn confirm_model_removal(confirmed: bool) -> signal_core::Result<()> {
    if confirmed {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(SignalError::InvalidConfiguration(
            "noninteractive model removal requires --yes".to_owned(),
        ));
    }
    eprint!("Remove this model profile? [y/N] ");
    std::io::stderr().flush()?;
    let mut response = String::new();
    std::io::stdin().read_line(&mut response)?;
    if matches!(response.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        Err(SignalError::InvalidConfiguration(
            "model profile removal was declined".to_owned(),
        ))
    }
}

fn model_add_input(args: ModelAddArgs) -> signal_core::Result<AddModelInput> {
    let ModelAddArgs {
        name,
        provider,
        model,
        endpoint,
        dialect,
        credential_env,
        max_summaries,
        daily_budget_usd,
        input_usd_per_million,
        output_usd_per_million,
        max_output_tokens,
        timeout_seconds,
        max_retries,
        consent_provider_data_sharing,
    } = args;
    let provider = provider.into();
    let dialect = dialect.map(Into::into);
    let endpoint = endpoint
        .map(|value| {
            value.parse::<url::Url>().map_err(|_| {
                SignalError::InvalidConfiguration("custom endpoint URL is invalid".to_owned())
            })
        })
        .transpose()?;
    let defaults = ProfileLimits::default();
    let limits = ProfileLimits {
        max_summaries_per_refresh: max_summaries.unwrap_or(defaults.max_summaries_per_refresh),
        max_daily_cost_microusd: parse_optional_usd(daily_budget_usd.as_deref())?,
        input_cost_microusd_per_million: parse_optional_usd(input_usd_per_million.as_deref())?,
        output_cost_microusd_per_million: parse_optional_usd(output_usd_per_million.as_deref())?,
        max_output_tokens: max_output_tokens.unwrap_or(defaults.max_output_tokens),
        timeout_seconds: timeout_seconds.unwrap_or(defaults.timeout_seconds),
        max_retries: max_retries.unwrap_or(defaults.max_retries),
    };

    NewModelProfile {
        name: name.clone(),
        provider,
        model: model.clone(),
        endpoint: endpoint.clone(),
        dialect,
        credential: CredentialRef::Environment {
            variable: "SIGNAL_CREDENTIAL_VALIDATION".to_owned(),
        },
        consented_at: Some(Utc::now()),
        enabled: true,
        limits: limits.clone(),
    }
    .validate()?;

    let interactive = std::io::stdin().is_terminal();
    let consented_at = confirm_provider_data_sharing(consent_provider_data_sharing, interactive)?;
    let credential = match credential_env {
        Some(variable) => AddModelCredential::Environment { variable },
        None if interactive => {
            let secret = rpassword::prompt_password("Credential: ")?;
            AddModelCredential::SystemStore {
                secret: secret.into(),
            }
        }
        None => {
            return Err(SignalError::InvalidConfiguration(
                "noninteractive model creation requires a credential environment reference"
                    .to_owned(),
            ));
        }
    };

    Ok(AddModelInput {
        name,
        provider,
        model,
        endpoint,
        dialect,
        credential,
        consented_at: Some(consented_at),
        enabled: true,
        limits,
    })
}

fn parse_optional_usd(value: Option<&str>) -> signal_core::Result<Option<u64>> {
    value
        .map(MoneyMicros::parse_usd)
        .transpose()
        .map(|value| value.map(MoneyMicros::as_micros))
}

fn confirm_provider_data_sharing(
    consented: bool,
    interactive: bool,
) -> signal_core::Result<chrono::DateTime<Utc>> {
    confirm_provider_data_sharing_with(consented, interactive, || {
        eprintln!(
            "Provider data-sharing disclosure\nAI summary generation sends the selected story's title, excerpt, canonical URL, publication time, category, and source IDs to the configured provider."
        );
        eprint!("Consent to this provider data sharing? [y/N] ");
        std::io::stderr().flush()?;
        let mut response = String::new();
        std::io::stdin().read_line(&mut response)?;
        Ok(response)
    })
}

fn confirm_provider_data_sharing_with(
    consented: bool,
    interactive: bool,
    disclose_and_read: impl FnOnce() -> signal_core::Result<String>,
) -> signal_core::Result<chrono::DateTime<Utc>> {
    if !interactive {
        return if consented {
            Ok(Utc::now())
        } else {
            Err(SignalError::InvalidConfiguration(
                "explicit provider data-sharing consent is required".to_owned(),
            ))
        };
    }

    let response = disclose_and_read()?;
    if matches!(response.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(Utc::now())
    } else {
        Err(SignalError::InvalidConfiguration(
            "provider data-sharing consent was declined".to_owned(),
        ))
    }
}

impl From<ProviderArg> for ProviderKind {
    fn from(value: ProviderArg) -> Self {
        match value {
            ProviderArg::OpenAi => Self::OpenAi,
            ProviderArg::Anthropic => Self::Anthropic,
            ProviderArg::Gemini => Self::Gemini,
            ProviderArg::OpenAiCompatible => Self::OpenAiCompatible,
        }
    }
}

impl From<DialectArg> for ApiDialect {
    fn from(value: DialectArg) -> Self {
        match value {
            DialectArg::Responses => Self::Responses,
            DialectArg::ChatCompletions => Self::ChatCompletions,
        }
    }
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

fn exit_code(error: &CliError) -> i32 {
    match error {
        CliError::ExplicitGeneration(_) => 6,
        CliError::Core(error) => match error {
            SignalError::InvalidConfiguration(_)
            | SignalError::Io(_)
            | SignalError::Serialization(_) => 2,
            SignalError::Network(_)
            | SignalError::Feed(_)
            | SignalError::Refresh(_)
            | SignalError::Cancelled
            | SignalError::Provider(_) => 3,
            SignalError::NotFound(_) => 4,
            SignalError::Database(_) | SignalError::Storage(_) | SignalError::Credential(_) => 5,
        },
    }
}

fn display_error(error: &CliError) -> String {
    match error {
        CliError::ExplicitGeneration(status) => match status {
            ManualGenerationStatus::BudgetExhausted => {
                "AI generation failed: budget exhausted".to_owned()
            }
            ManualGenerationStatus::CredentialUnavailable => {
                "AI generation failed: credential unavailable".to_owned()
            }
            _ => "AI generation failed".to_owned(),
        },
        CliError::Core(error) => match error {
            SignalError::InvalidConfiguration(message) => format!("Configuration error: {message}"),
            SignalError::Io(_) | SignalError::Serialization(_) => {
                "Configuration could not be read or written".to_owned()
            }
            SignalError::Network(_) | SignalError::Feed(_) | SignalError::Refresh(_) => {
                "Refresh failed".to_owned()
            }
            SignalError::Cancelled => "Refresh cancelled".to_owned(),
            SignalError::Provider(_) => "AI provider request failed".to_owned(),
            SignalError::NotFound(message) => message.clone(),
            SignalError::Database(_) | SignalError::Storage(_) => {
                "Storage operation failed".to_owned()
            }
            SignalError::Credential(_) => "Credential operation failed".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn interactive_consent_flag_still_requires_disclosure_and_yes() {
        let disclosures = Cell::new(0);

        let consent = confirm_provider_data_sharing_with(true, true, || {
            disclosures.set(disclosures.get() + 1);
            Ok("yes".to_owned())
        });

        assert!(consent.is_ok());
        assert_eq!(disclosures.get(), 1);
    }

    #[test]
    fn interactive_consent_refusal_is_rejected_after_disclosure() {
        let disclosures = Cell::new(0);

        let consent = confirm_provider_data_sharing_with(true, true, || {
            disclosures.set(disclosures.get() + 1);
            Ok("no".to_owned())
        });

        assert!(matches!(
            consent,
            Err(SignalError::InvalidConfiguration(message))
                if message == "provider data-sharing consent was declined"
        ));
        assert_eq!(disclosures.get(), 1);
    }

    #[test]
    fn cancelled_core_operation_uses_refresh_exit_and_message() {
        // Break caught: exposing a cancellation as an unrelated CLI failure.
        let error = CliError::Core(SignalError::Cancelled);

        assert_eq!(exit_code(&error), 3);
        assert_eq!(display_error(&error), "Refresh cancelled");
    }
}
