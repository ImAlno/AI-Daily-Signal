use std::{path::PathBuf, time::Duration};

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{Briefing, BriefingItem, Result, ScoreBreakdown, SignalError, Story};

const INITIAL_MIGRATION: &str = include_str!("../migrations/001_initial.sql");
const ITEM_STALENESS_MIGRATION: &str =
    include_str!("../migrations/002_briefing_item_staleness.sql");

#[derive(Clone, Debug)]
pub struct Store {
    path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreStatus {
    pub story_count: u64,
    pub last_refresh_at: Option<DateTime<Utc>>,
    pub data_generation: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefreshRun {
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub successful_sources: u64,
    pub failed_sources: u64,
}

impl Store {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }

        let store = Self { path };
        store.apply_migrations()?;
        Ok(store)
    }

    pub fn upsert_stories(&self, stories: &[Story]) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        upsert_stories_in_transaction(&transaction, stories)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn commit_refresh(&self, stories: &[Story], briefing: &Briefing) -> Result<()> {
        self.commit_refresh_transaction(stories, briefing, None)
    }

    pub fn commit_refresh_with_counts(
        &self,
        stories: &[Story],
        briefing: &Briefing,
        successful_sources: usize,
        failed_sources: usize,
    ) -> Result<()> {
        let successful_sources = count_as_i64(successful_sources)?;
        let failed_sources = count_as_i64(failed_sources)?;
        self.commit_refresh_transaction(
            stories,
            briefing,
            Some((successful_sources, failed_sources)),
        )
    }

    fn commit_refresh_transaction(
        &self,
        stories: &[Story],
        briefing: &Briefing,
        source_counts: Option<(i64, i64)>,
    ) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

        upsert_stories_in_transaction(&transaction, stories)?;
        transaction.execute(
            "INSERT INTO briefings (date, generated_at) VALUES (?1, ?2)
             ON CONFLICT(date) DO UPDATE SET generated_at = excluded.generated_at",
            params![
                briefing.date.to_string(),
                briefing.generated_at.to_rfc3339()
            ],
        )?;
        transaction.execute(
            "DELETE FROM briefing_items WHERE briefing_date = ?1",
            [briefing.date.to_string()],
        )?;
        for item in &briefing.items {
            transaction.execute(
                "INSERT INTO briefing_items (
                     briefing_date, story_id, position, section, is_stale
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    briefing.date.to_string(),
                    item.story.id,
                    i64::from(item.position),
                    item.section,
                    item.is_stale
                ],
            )?;
        }
        transaction.execute(
            "UPDATE metadata SET value = CAST(value AS INTEGER) + 1
             WHERE key = 'data_generation'",
            [],
        )?;
        if let Some((successful_sources, failed_sources)) = source_counts {
            insert_refresh_run(
                &transaction,
                briefing.generated_at,
                successful_sources,
                failed_sources,
                None,
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    pub fn record_refresh_failure(
        &self,
        occurred_at: DateTime<Utc>,
        failed_sources: usize,
    ) -> Result<()> {
        let failed_sources = count_as_i64(failed_sources)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        insert_refresh_run(
            &transaction,
            occurred_at,
            0,
            failed_sources,
            Some(r#"{"kind":"all_sources_failed"}"#),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn latest_refresh_run(&self) -> Result<Option<RefreshRun>> {
        let connection = self.connect()?;
        let stored = connection
            .query_row(
                "SELECT started_at, finished_at, successful_sources, failed_sources
                 FROM refresh_runs
                 ORDER BY id DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;

        stored
            .map(
                |(started_at, finished_at, successful_sources, failed_sources)| {
                    Ok(RefreshRun {
                        started_at: parse_datetime(&started_at)?,
                        finished_at: finished_at
                            .map(|value| parse_datetime(&value))
                            .transpose()?,
                        successful_sources: count_as_u64(successful_sources)?,
                        failed_sources: count_as_u64(failed_sources)?,
                    })
                },
            )
            .transpose()
    }

    pub fn load_briefing(&self, date: NaiveDate) -> Result<Option<Briefing>> {
        let connection = self.connect()?;
        let generated_at = connection
            .query_row(
                "SELECT generated_at FROM briefings WHERE date = ?1",
                [date.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        let Some(generated_at) = generated_at else {
            return Ok(None);
        };

        let mut statement = connection.prepare(
            "SELECT bi.position, bi.section, bi.is_stale,
                    s.id, s.title, s.canonical_url, s.excerpt, s.category, s.published_at,
                    s.source_ids_json, s.score_json, s.smart_summary, s.is_read, s.is_saved
             FROM briefing_items bi
             JOIN stories s ON s.id = bi.story_id
             WHERE bi.briefing_date = ?1
             ORDER BY bi.position ASC",
        )?;
        let rows = statement.query_map([date.to_string()], briefing_item_row)?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?.into_briefing_item()?);
        }

        Ok(Some(Briefing {
            date,
            generated_at: parse_datetime(&generated_at)?,
            items,
        }))
    }

    pub fn load_latest_briefing(&self) -> Result<Option<Briefing>> {
        let connection = self.connect()?;
        let date = connection
            .query_row(
                "SELECT date FROM briefings
                 ORDER BY generated_at DESC, date DESC
                 LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(date) = date else {
            return Ok(None);
        };
        let date = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .map_err(|error| SignalError::Serialization(error.to_string()))?;
        drop(connection);
        self.load_briefing(date)
    }

    pub fn list_latest(&self) -> Result<Vec<Story>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, title, canonical_url, excerpt, category, published_at,
                    source_ids_json, score_json, smart_summary, is_read, is_saved
             FROM stories
             ORDER BY published_at IS NULL ASC, published_at DESC, updated_at DESC, id ASC",
        )?;
        collect_stories(statement.query_map([], story_row)?)
    }

    pub fn list_saved(&self) -> Result<Vec<Story>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, title, canonical_url, excerpt, category, published_at,
                    source_ids_json, score_json, smart_summary, is_read, is_saved
             FROM stories
             WHERE is_saved = 1
             ORDER BY published_at IS NULL ASC, published_at DESC, updated_at DESC, id ASC",
        )?;
        collect_stories(statement.query_map([], story_row)?)
    }

    pub fn find_story(&self, id: &str) -> Result<Option<Story>> {
        let connection = self.connect()?;
        let stored = connection
            .query_row(
                "SELECT id, title, canonical_url, excerpt, category, published_at,
                        source_ids_json, score_json, smart_summary, is_read, is_saved
                 FROM stories WHERE id = ?1",
                [id],
                story_row,
            )
            .optional()?;
        stored.map(StoredStory::into_story).transpose()
    }

    pub fn set_saved(&self, id: &str, saved: bool) -> Result<()> {
        let connection = self.connect()?;
        let changed = connection.execute(
            "UPDATE stories SET is_saved = ?1 WHERE id = ?2",
            params![saved, id],
        )?;
        if changed == 0 {
            return Err(SignalError::NotFound(format!("story {id}")));
        }
        Ok(())
    }

    pub fn status(&self) -> Result<StoreStatus> {
        let connection = self.connect()?;
        let story_count = connection.query_row("SELECT COUNT(*) FROM stories", [], |row| {
            row.get::<_, i64>(0)
        })?;
        let last_refresh_at = connection
            .query_row("SELECT MAX(generated_at) FROM briefings", [], |row| {
                row.get::<_, Option<String>>(0)
            })?
            .map(|value| parse_datetime(&value))
            .transpose()?;
        let data_generation = connection.query_row(
            "SELECT value FROM metadata WHERE key = 'data_generation'",
            [],
            |row| row.get::<_, String>(0),
        )?;

        Ok(StoreStatus {
            story_count: u64::try_from(story_count)
                .map_err(|error| SignalError::Serialization(error.to_string()))?,
            last_refresh_at,
            data_generation: data_generation.parse().map_err(
                |error: std::num::ParseIntError| SignalError::Serialization(error.to_string()),
            )?,
        })
    }

    pub fn journal_mode(&self) -> Result<String> {
        let connection = self.connect()?;
        connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(Into::into)
    }

    fn connect(&self) -> Result<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        Ok(connection)
    }

    fn apply_migrations(&self) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(INITIAL_MIGRATION)?;
        transaction.execute(
            "INSERT OR IGNORE INTO schema_migrations (version, applied_at)
             VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            [],
        )?;
        let has_item_staleness = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 2)",
            [],
            |row| row.get::<_, bool>(0),
        )?;
        if !has_item_staleness {
            transaction.execute_batch(ITEM_STALENESS_MIGRATION)?;
            transaction.execute(
                "INSERT INTO schema_migrations (version, applied_at)
                 VALUES (2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                [],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}

fn insert_refresh_run(
    transaction: &Transaction<'_>,
    occurred_at: DateTime<Utc>,
    successful_sources: i64,
    failed_sources: i64,
    error_json: Option<&str>,
) -> Result<()> {
    let occurred_at = occurred_at.to_rfc3339();
    transaction.execute(
        "INSERT INTO refresh_runs (
             started_at, finished_at, successful_sources, failed_sources, error_json
         ) VALUES (?1, ?1, ?2, ?3, ?4)",
        params![occurred_at, successful_sources, failed_sources, error_json],
    )?;
    Ok(())
}

fn count_as_i64(count: usize) -> Result<i64> {
    i64::try_from(count).map_err(|error| SignalError::Serialization(error.to_string()))
}

fn count_as_u64(count: i64) -> Result<u64> {
    u64::try_from(count).map_err(|error| SignalError::Serialization(error.to_string()))
}

fn upsert_stories_in_transaction(transaction: &Transaction<'_>, stories: &[Story]) -> Result<()> {
    let updated_at = Utc::now().to_rfc3339();
    let mut statement = transaction.prepare(
        "INSERT INTO stories (
             id, title, canonical_url, excerpt, category, published_at, source_ids_json,
             score_json, smart_summary, is_read, is_saved, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
         ON CONFLICT(id) DO UPDATE SET
             title = excluded.title,
             canonical_url = excluded.canonical_url,
             excerpt = excluded.excerpt,
             category = excluded.category,
             published_at = excluded.published_at,
             source_ids_json = excluded.source_ids_json,
             score_json = excluded.score_json,
             smart_summary = excluded.smart_summary,
             updated_at = excluded.updated_at",
    )?;

    for story in stories {
        let source_ids = serialize(&story.source_ids)?;
        let score = serialize(&story.score)?;
        statement.execute(params![
            story.id,
            story.title,
            story.canonical_url,
            story.excerpt,
            story.category,
            story.published_at.map(|value| value.to_rfc3339()),
            source_ids,
            score,
            story.smart_summary,
            story.is_read,
            story.is_saved,
            updated_at,
        ])?;
    }
    Ok(())
}

fn serialize<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|error| SignalError::Serialization(error.to_string()))
}

fn deserialize<T: for<'de> Deserialize<'de>>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|error| SignalError::Serialization(error.to_string()))
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| SignalError::Serialization(error.to_string()))
}

fn collect_stories(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<StoredStory>>,
) -> Result<Vec<Story>> {
    let mut stories = Vec::new();
    for row in rows {
        stories.push(row?.into_story()?);
    }
    Ok(stories)
}

fn story_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredStory> {
    Ok(StoredStory {
        id: row.get(0)?,
        title: row.get(1)?,
        canonical_url: row.get(2)?,
        excerpt: row.get(3)?,
        category: row.get(4)?,
        published_at: row.get(5)?,
        source_ids_json: row.get(6)?,
        score_json: row.get(7)?,
        smart_summary: row.get(8)?,
        is_read: row.get(9)?,
        is_saved: row.get(10)?,
    })
}

fn briefing_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredBriefingItem> {
    Ok(StoredBriefingItem {
        position: row.get(0)?,
        section: row.get(1)?,
        is_stale: row.get(2)?,
        story: StoredStory {
            id: row.get(3)?,
            title: row.get(4)?,
            canonical_url: row.get(5)?,
            excerpt: row.get(6)?,
            category: row.get(7)?,
            published_at: row.get(8)?,
            source_ids_json: row.get(9)?,
            score_json: row.get(10)?,
            smart_summary: row.get(11)?,
            is_read: row.get(12)?,
            is_saved: row.get(13)?,
        },
    })
}

struct StoredStory {
    id: String,
    title: String,
    canonical_url: String,
    excerpt: String,
    category: String,
    published_at: Option<String>,
    source_ids_json: String,
    score_json: String,
    smart_summary: String,
    is_read: bool,
    is_saved: bool,
}

impl StoredStory {
    fn into_story(self) -> Result<Story> {
        Ok(Story {
            id: self.id,
            title: self.title,
            canonical_url: self.canonical_url,
            excerpt: self.excerpt,
            category: self.category,
            published_at: self
                .published_at
                .map(|value| parse_datetime(&value))
                .transpose()?,
            source_ids: deserialize(&self.source_ids_json)?,
            score: deserialize::<ScoreBreakdown>(&self.score_json)?,
            smart_summary: self.smart_summary,
            is_read: self.is_read,
            is_saved: self.is_saved,
        })
    }
}

struct StoredBriefingItem {
    position: u32,
    section: String,
    is_stale: bool,
    story: StoredStory,
}

impl StoredBriefingItem {
    fn into_briefing_item(self) -> Result<BriefingItem> {
        Ok(BriefingItem {
            position: self.position,
            section: self.section,
            is_stale: self.is_stale,
            story: self.story.into_story()?,
        })
    }
}
