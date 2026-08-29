use signal_core::{Briefing, RefreshReport, Source, StoreStatus, Story, TodayView};

#[derive(serde::Serialize)]
pub struct JsonEnvelope<T> {
    pub schema_version: u32,
    pub data: T,
}

impl<T> JsonEnvelope<T> {
    pub fn new(data: T) -> Self {
        Self {
            schema_version: 1,
            data,
        }
    }
}

pub fn json<T: serde::Serialize>(data: T) -> signal_core::Result<String> {
    serde_json::to_string_pretty(&JsonEnvelope::new(data))
        .map_err(|error| signal_core::SignalError::Serialization(error.to_string()))
}

pub fn status(status: &StoreStatus) -> String {
    format!(
        "Stories: {}\nLast refresh: {}\nData generation: {}",
        status.story_count,
        status
            .last_refresh_at
            .map_or_else(|| "never".to_owned(), |value| value.to_rfc3339()),
        status.data_generation
    )
}

pub fn briefing(briefing: &Briefing) -> String {
    let mut rendered = format!(
        "Briefing for {}\nGenerated: {}\n",
        briefing.date,
        briefing.generated_at.to_rfc3339()
    );
    for item in &briefing.items {
        let saved = if item.story.is_saved { " [saved]" } else { "" };
        let stale = if item.is_stale { " [stale]" } else { "" };
        rendered.push_str(&format!(
            "\n{}. {}{}{}\n{}\n{}\n",
            item.position,
            item.story.title,
            saved,
            stale,
            item.story.smart_summary,
            item.story.canonical_url
        ));
    }
    rendered
}

pub fn today(view: &TodayView) -> String {
    let status = if view.is_stale { "stale" } else { "fresh" };
    format!("Status: {status}\n{}", briefing(&view.briefing))
}

pub fn refresh(report: &RefreshReport) -> String {
    format!(
        "Refreshed from {} source(s); {} failed\n{}",
        report.successful_sources,
        report.failures.len(),
        briefing(&report.briefing)
    )
}

pub fn story(story: &Story) -> String {
    let saved = if story.is_saved { " [saved]" } else { "" };
    format!(
        "{}{}\n{}\n{}",
        story.title, saved, story.smart_summary, story.canonical_url
    )
}

pub fn stories(stories: &[Story]) -> String {
    if stories.is_empty() {
        return "No stories stored".to_owned();
    }

    stories
        .iter()
        .enumerate()
        .map(|(index, story)| {
            let saved = if story.is_saved { " [saved]" } else { "" };
            format!(
                "{}. {}{}\n{}\n{}",
                index + 1,
                story.title,
                saved,
                story.smart_summary,
                story.canonical_url
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn sources(sources: &[Source]) -> String {
    sources
        .iter()
        .map(|source| {
            let state = if source.enabled {
                "enabled"
            } else {
                "disabled"
            };
            format!("{}\t{}\t{}", source.id, state, source.name)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
