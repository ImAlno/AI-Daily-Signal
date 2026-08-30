use std::{collections::BTreeMap, sync::Arc};

use chrono::Utc;
use signal_core::{SignalApp, SignalError};
use tokio::sync::Mutex;

use crate::{
    CompanionError, CompanionSnapshot, FfiBriefing, FfiCollectionStatus, FfiModelProfile,
    FfiSource, FfiStateRevision, FfiStory, FfiSummaryVariant, types,
};

#[derive(uniffi::Object)]
pub struct CompanionClient {
    app: Mutex<SignalApp>,
}

#[uniffi::export(async_runtime = "tokio")]
impl CompanionClient {
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, CompanionError> {
        Ok(Arc::new(Self {
            app: Mutex::new(SignalApp::open().map_err(CompanionError::from)?),
        }))
    }

    pub async fn snapshot(&self) -> Result<CompanionSnapshot, CompanionError> {
        let mut app = self.app.lock().await;
        let revision = app.state_revision().map_err(CompanionError::from)?.into();
        let status = FfiCollectionStatus::from(app.status().map_err(CompanionError::from)?);
        let today = today_snapshot(&app)?;
        let selected_summaries = today
            .iter()
            .flat_map(|briefing| &briefing.items)
            .filter_map(|item| {
                item.selected_summary
                    .clone()
                    .map(|summary| (item.story.id.clone(), summary))
            })
            .collect::<BTreeMap<_, _>>();
        let latest = app
            .latest(usize::MAX)
            .map_err(CompanionError::from)?
            .into_iter()
            .map(|story| {
                let selected = selected_summaries.get(&story.id).cloned();
                story_snapshot(&app, story, selected)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let saved = app
            .saved()
            .map_err(CompanionError::from)?
            .into_iter()
            .map(|story| {
                let selected = selected_summaries.get(&story.id).cloned();
                story_snapshot(&app, story, selected)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let sources = app
            .list_source_records()
            .map_err(CompanionError::from)?
            .into_iter()
            .map(FfiSource::from)
            .collect();
        let model_profiles = app.list_models().map_err(CompanionError::from)?;
        let has_usable_ai_profile = model_profiles
            .iter()
            .any(|profile| profile.enabled && profile.consented_at.is_some());
        let model_profiles = model_profiles
            .into_iter()
            .map(FfiModelProfile::from)
            .collect();
        let default_model_profile_id = app
            .default_model_profile()
            .map_err(CompanionError::from)?
            .map(|profile| profile.id.hyphenated().to_string());

        Ok(CompanionSnapshot {
            revision,
            status,
            today,
            latest,
            saved,
            sources,
            model_profiles,
            default_model_profile_id,
            has_usable_ai_profile,
        })
    }

    pub async fn state_revision(&self) -> Result<FfiStateRevision, CompanionError> {
        self.app
            .lock()
            .await
            .state_revision()
            .map(Into::into)
            .map_err(CompanionError::from)
    }
}

impl CompanionClient {
    #[cfg(feature = "test-support")]
    pub fn for_test(app: SignalApp) -> Arc<Self> {
        Arc::new(Self {
            app: Mutex::new(app),
        })
    }
}

fn today_snapshot(app: &SignalApp) -> Result<Option<FfiBriefing>, CompanionError> {
    let today = match app.today(Utc::now()) {
        Ok(today) => today,
        Err(SignalError::NotFound(_)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let summary_variants = today
        .briefing
        .items
        .iter()
        .map(|item| {
            app.summary_variants(&item.story.id)
                .map(|variants| variants.into_iter().map(FfiSummaryVariant::from).collect())
                .map_err(CompanionError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(types::briefing(today, summary_variants)))
}

fn story_snapshot(
    app: &SignalApp,
    story: signal_core::Story,
    selected_summary: Option<FfiSummaryVariant>,
) -> Result<FfiStory, CompanionError> {
    let summary_variants = app
        .summary_variants(&story.id)
        .map_err(CompanionError::from)?
        .into_iter()
        .map(FfiSummaryVariant::from)
        .collect();
    Ok(types::story(story, selected_summary, summary_variants))
}
