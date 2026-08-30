use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use secrecy::SecretString;
use signal_core::{
    AddModelCredential, AddModelInput, CancellationToken, CredentialWarningKind,
    ManualGenerationStatus, NewFeedSource, RefreshOptions, RefreshReport, SignalApp, SignalError,
    SummarizeOptions, TestModelOptions, TodayView,
};
use tokio::sync::Mutex as AsyncMutex;

use crate::{
    AddCredentialRequest, AddFeedSourceRequest, AddModelProfileRequest, CompanionError,
    CompanionSnapshot, FfiBriefing, FfiCollectionStatus, FfiCredentialDeletionStatus,
    FfiModelMutation, FfiModelProfile, FfiModelRemoval, FfiModelTestMutation, FfiRefreshResult,
    FfiSource, FfiSourceMutation, FfiStateRevision, FfiStory, FfiStoryMutation, FfiSummaryVariant,
    types,
};

struct ActiveRefresh {
    id: String,
    cancellation: CancellationToken,
}

struct ActiveRefreshGuard<'a> {
    active_refresh: &'a Mutex<Option<ActiveRefresh>>,
    id: String,
}

impl Drop for ActiveRefreshGuard<'_> {
    fn drop(&mut self) {
        let mut active = self
            .active_refresh
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active.as_ref().is_some_and(|refresh| refresh.id == self.id) {
            *active = None;
        }
    }
}

#[derive(uniffi::Object)]
pub struct CompanionClient {
    app: AsyncMutex<SignalApp>,
    active_refresh: Mutex<Option<ActiveRefresh>>,
}

#[uniffi::export(async_runtime = "tokio")]
impl CompanionClient {
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, CompanionError> {
        Ok(Arc::new(Self {
            app: AsyncMutex::new(SignalApp::open().map_err(CompanionError::from)?),
            active_refresh: Mutex::new(None),
        }))
    }

    pub async fn refresh(
        &self,
        operation_id: String,
        ai: bool,
    ) -> Result<FfiRefreshResult, CompanionError> {
        let (cancellation, _active_refresh) = self.begin_refresh(operation_id)?;
        let mut app = self.app.lock().await;
        match app
            .refresh_with_control(Utc::now(), RefreshOptions { ai }, &cancellation)
            .await
        {
            Ok(report) => refresh_result(&mut app, report),
            Err(error) => Err(error.into()),
        }
    }

    pub fn cancel_operation(&self, operation_id: String) -> bool {
        let active = self
            .active_refresh
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(refresh) = active.as_ref().filter(|refresh| refresh.id == operation_id) else {
            return false;
        };
        refresh.cancellation.cancel();
        true
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
            .map(FfiSource::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let has_usable_ai_profile = app.has_usable_ai_profile().map_err(CompanionError::from)?;
        let model_profiles = app.list_models().map_err(CompanionError::from)?;
        let model_profiles = model_profiles
            .into_iter()
            .map(FfiModelProfile::try_from)
            .collect::<Result<Vec<_>, _>>()?;
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

    pub async fn set_story_saved(
        &self,
        id: String,
        saved: bool,
    ) -> Result<FfiStoryMutation, CompanionError> {
        let mut app = self.app.lock().await;
        let story = app.set_saved(&id, saved).map_err(CompanionError::from)?;
        let selected_summary = selected_summary_for_story(&app, &id)?;
        story_mutation(&mut app, story, selected_summary)
    }

    pub async fn set_story_read(
        &self,
        id: String,
        read: bool,
    ) -> Result<FfiStoryMutation, CompanionError> {
        let mut app = self.app.lock().await;
        let story = app.set_read(&id, read).map_err(CompanionError::from)?;
        let selected_summary = selected_summary_for_story(&app, &id)?;
        story_mutation(&mut app, story, selected_summary)
    }

    pub async fn select_summary_variant(
        &self,
        story_id: String,
        variant_id: String,
    ) -> Result<FfiStoryMutation, CompanionError> {
        let variant_id = variant_id
            .parse()
            .map_err(|_| CompanionError::InvalidInput)?;
        let mut app = self.app.lock().await;
        let selected = app
            .select_summary_variant(&story_id, variant_id)
            .map_err(CompanionError::from)?;
        let story = app.show(&story_id).map_err(CompanionError::from)?;
        story_mutation(&mut app, story, Some(selected.into()))
    }

    pub async fn regenerate_story(
        &self,
        story_id: String,
        profile: Option<String>,
        force: bool,
    ) -> Result<FfiStoryMutation, CompanionError> {
        let mut app = self.app.lock().await;
        let report = app
            .summarize_story(&story_id, SummarizeOptions { profile, force }, Utc::now())
            .await
            .map_err(CompanionError::from)?;
        ensure_generation_succeeded(report.status)?;
        let selected = report.summary.map(FfiSummaryVariant::from);
        let story = app.show(&story_id).map_err(CompanionError::from)?;
        story_mutation(&mut app, story, selected)
    }

    pub async fn add_feed_source(
        &self,
        request: AddFeedSourceRequest,
    ) -> Result<FfiSourceMutation, CompanionError> {
        let mut app = self.app.lock().await;
        let mutation = app
            .add_feed_source_with_revision(NewFeedSource {
                name: request.name,
                category: request.category,
                url: request.url,
                weight: request.weight,
                enabled: request.enabled,
            })
            .map_err(CompanionError::from)?;
        source_mutation(
            &mut app,
            mutation.value.try_into()?,
            mutation.source_config_revision,
        )
    }

    pub async fn set_source_enabled(
        &self,
        id: String,
        enabled: bool,
    ) -> Result<FfiSourceMutation, CompanionError> {
        let mut app = self.app.lock().await;
        let mutation = app
            .set_source_enabled_with_revision(&id, enabled)
            .map_err(CompanionError::from)?;
        source_mutation(
            &mut app,
            mutation.value.try_into()?,
            mutation.source_config_revision,
        )
    }

    pub async fn remove_personal_source(
        &self,
        id: String,
    ) -> Result<FfiSourceMutation, CompanionError> {
        let mut app = self.app.lock().await;
        let mutation = app
            .remove_personal_source_with_revision(&id)
            .map_err(CompanionError::from)?;
        source_mutation(
            &mut app,
            mutation.value.try_into()?,
            mutation.source_config_revision,
        )
    }

    pub async fn add_model_profile(
        &self,
        request: AddModelProfileRequest,
    ) -> Result<FfiModelMutation, CompanionError> {
        let AddModelProfileRequest {
            name,
            provider,
            model,
            endpoint,
            dialect,
            credential,
            consent_provider_data_sharing,
            limits,
        } = request;
        let credential = match credential {
            AddCredentialRequest::SystemStore { secret } => AddModelCredential::SystemStore {
                secret: SecretString::from(secret),
            },
            AddCredentialRequest::Environment { variable } => {
                AddModelCredential::Environment { variable }
            }
        };
        let endpoint = endpoint
            .map(|value| value.parse())
            .transpose()
            .map_err(|_| CompanionError::InvalidInput)?;
        let limits = limits.try_into()?;
        let input = AddModelInput {
            name,
            provider: provider.into(),
            model,
            endpoint,
            dialect: dialect.map(Into::into),
            credential,
            consented_at: consent_provider_data_sharing.then(Utc::now),
            enabled: true,
            limits,
        };
        let mut app = self.app.lock().await;
        let report = app.add_model(input, Utc::now()).map_err(|error| {
            if !consent_provider_data_sharing
                && matches!(error, SignalError::InvalidConfiguration(_))
            {
                CompanionError::ConsentRequired
            } else {
                CompanionError::from(error)
            }
        })?;
        model_mutation(&mut app, report.profile.try_into()?)
    }

    pub async fn set_default_model_profile(
        &self,
        profile: String,
    ) -> Result<FfiModelMutation, CompanionError> {
        let mut app = self.app.lock().await;
        let profile = app.use_model(&profile).map_err(CompanionError::from)?;
        model_mutation(&mut app, profile.try_into()?)
    }

    pub async fn test_model_profile(
        &self,
        profile: String,
    ) -> Result<FfiModelTestMutation, CompanionError> {
        let mut app = self.app.lock().await;
        let report = app
            .test_model(TestModelOptions { profile }, Utc::now())
            .await
            .map_err(CompanionError::from)?;
        ensure_generation_succeeded(report.status)?;
        let profile = app
            .list_models()
            .map_err(CompanionError::from)?
            .into_iter()
            .find(|profile| profile.id == report.profile_id)
            .ok_or(CompanionError::StorageUnavailable)?
            .try_into()?;
        let revision = app.state_revision().map_err(CompanionError::from)?.into();
        Ok(FfiModelTestMutation {
            profile,
            cost_may_apply: report.cost_may_apply,
            revision,
        })
    }

    pub async fn remove_model_profile(
        &self,
        profile: String,
    ) -> Result<FfiModelRemoval, CompanionError> {
        let mut app = self.app.lock().await;
        let profiles = app.list_models().map_err(CompanionError::from)?;
        let report = app.remove_model(&profile).map_err(CompanionError::from)?;
        let removed = profiles
            .into_iter()
            .find(|profile| profile.id == report.removed_profile_id)
            .ok_or(CompanionError::StorageUnavailable)?;
        let credential_deletion = match (report.credential_deleted, report.warning) {
            (_, Some(CredentialWarningKind::DeleteFailed)) => {
                FfiCredentialDeletionStatus::DeleteFailed
            }
            (true, None) => FfiCredentialDeletionStatus::Deleted,
            (false, None) => FfiCredentialDeletionStatus::NotApplicable,
        };
        let profile = removed.try_into()?;
        let revision = app.state_revision().map_err(CompanionError::from)?.into();
        Ok(FfiModelRemoval {
            profile,
            credential_deletion,
            revision,
        })
    }
}

impl CompanionClient {
    fn begin_refresh(
        &self,
        id: String,
    ) -> Result<(CancellationToken, ActiveRefreshGuard<'_>), CompanionError> {
        let cancellation = CancellationToken::new();
        let mut active = self
            .active_refresh
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active.is_some() {
            return Err(CompanionError::RefreshAlreadyRunning);
        }
        *active = Some(ActiveRefresh {
            id: id.clone(),
            cancellation: cancellation.clone(),
        });
        drop(active);
        Ok((
            cancellation,
            ActiveRefreshGuard {
                active_refresh: &self.active_refresh,
                id,
            },
        ))
    }

    #[cfg(feature = "test-support")]
    pub fn for_test(app: SignalApp) -> Arc<Self> {
        Arc::new(Self {
            app: AsyncMutex::new(app),
            active_refresh: Mutex::new(None),
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
    Ok(Some(types::briefing(today, summary_variants)?))
}

fn refresh_result(
    app: &mut SignalApp,
    report: RefreshReport,
) -> Result<FfiRefreshResult, CompanionError> {
    let successful_sources =
        u64::try_from(report.successful_sources).map_err(|_| CompanionError::StorageUnavailable)?;
    let failed_sources =
        u64::try_from(report.failures.len()).map_err(|_| CompanionError::StorageUnavailable)?;
    let summary_variants = report
        .briefing
        .items
        .iter()
        .map(|item| {
            app.summary_variants(&item.story.id)
                .map(|variants| variants.into_iter().map(FfiSummaryVariant::from).collect())
                .map_err(CompanionError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let briefing = types::briefing(TodayView::fresh(report.briefing), summary_variants)?;
    let generation = report.generation.try_into()?;
    let revision = app.state_revision().map_err(CompanionError::from)?.into();
    Ok(FfiRefreshResult {
        briefing,
        successful_sources,
        failed_sources,
        generation,
        revision,
    })
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
    types::story(story, selected_summary, summary_variants)
}

fn selected_summary_for_story(
    app: &SignalApp,
    story_id: &str,
) -> Result<Option<FfiSummaryVariant>, CompanionError> {
    match app.today(Utc::now()) {
        Ok(today) => Ok(today
            .briefing
            .items
            .into_iter()
            .find(|item| item.story.id == story_id)
            .and_then(|item| item.selected_summary)
            .map(Into::into)),
        Err(SignalError::NotFound(_)) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn story_mutation(
    app: &mut SignalApp,
    story: signal_core::Story,
    selected_summary: Option<FfiSummaryVariant>,
) -> Result<FfiStoryMutation, CompanionError> {
    let story = story_snapshot(app, story, selected_summary)?;
    let revision = app.state_revision().map_err(CompanionError::from)?.into();
    Ok(FfiStoryMutation { story, revision })
}

fn source_mutation(
    app: &mut SignalApp,
    source: FfiSource,
    source_config_revision: String,
) -> Result<FfiSourceMutation, CompanionError> {
    let revision = FfiStateRevision {
        data_generation: app.status().map_err(CompanionError::from)?.data_generation,
        source_config_revision,
    };
    Ok(FfiSourceMutation { source, revision })
}

fn model_mutation(
    app: &mut SignalApp,
    profile: FfiModelProfile,
) -> Result<FfiModelMutation, CompanionError> {
    let revision = app.state_revision().map_err(CompanionError::from)?.into();
    Ok(FfiModelMutation { profile, revision })
}

fn ensure_generation_succeeded(status: ManualGenerationStatus) -> Result<(), CompanionError> {
    match status {
        ManualGenerationStatus::Generated | ManualGenerationStatus::CacheHit => Ok(()),
        ManualGenerationStatus::BudgetExhausted => Err(CompanionError::BudgetExhausted),
        ManualGenerationStatus::CredentialUnavailable => Err(CompanionError::CredentialUnavailable),
        ManualGenerationStatus::ConsentRequired => Err(CompanionError::ConsentRequired),
        ManualGenerationStatus::ProfileUnavailable | ManualGenerationStatus::RefreshCapReached => {
            Err(CompanionError::InvalidInput)
        }
        ManualGenerationStatus::ProviderFailure | ManualGenerationStatus::MalformedOutput => {
            Err(CompanionError::ProviderUnavailable)
        }
    }
}
