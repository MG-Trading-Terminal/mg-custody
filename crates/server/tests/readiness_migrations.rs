//! Migration-apply regression test.
//!
//! Runs every SQL migration against the configured DATABASE_URL and
//! asserts the three readiness-pipeline tables end up with the
//! columns the code reads. Protects against:
//!   - someone tweaking the SQL and forgetting to update the DB
//!     accessor (column name drift)
//!   - a migration that claims to CREATE TABLE but actually alters
//!     an existing one in a way that contradicts later code
//!
//! Gated on `DATABASE_URL`. Skip with a log line when unset — CI
//! that doesn't provision PG still passes.

use sqlx::PgPool;
use sqlx::Row;

async fn connect_or_skip() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    match PgPool::connect(&url).await {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("DATABASE_URL set but connect failed ({e}) — skipping");
            None
        }
    }
}

#[tokio::test]
async fn migrations_apply_cleanly_and_readiness_tables_have_expected_columns() {
    let Some(pool) = connect_or_skip().await else {
        return;
    };
    sqlx::migrate!("../db/migrations")
        .run(&pool)
        .await
        .expect("every migration in db/migrations must apply");

    // readiness_transitions (migration 036)
    let cols = sqlx::query(
        "SELECT column_name, data_type FROM information_schema.columns \
         WHERE table_name = 'readiness_transitions' ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let col_names: Vec<String> = cols
        .iter()
        .map(|r| r.get::<String, _>("column_name"))
        .collect();
    assert!(col_names.contains(&"id".to_string()));
    assert!(col_names.contains(&"feature".to_string()));
    assert!(col_names.contains(&"kind".to_string()));
    assert!(col_names.contains(&"reason".to_string()));
    assert!(col_names.contains(&"occurred_at".to_string()));

    // readiness_silences (migration 037 + 038 adds starts_at)
    let cols = sqlx::query(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'readiness_silences' ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let col_names: Vec<String> = cols
        .iter()
        .map(|r| r.get::<String, _>("column_name"))
        .collect();
    for expected in [
        "id",
        "feature",
        "starts_at",
        "expires_at",
        "reason",
        "created_by",
        "created_at",
    ] {
        assert!(
            col_names.iter().any(|n| n == expected),
            "readiness_silences missing column '{expected}' (has: {col_names:?})"
        );
    }

    // Exercise the insert+select paths end-to-end: write one
    // transition and one silence, read them back via the DB
    // helpers, assert the row comes back with the write's values.
    let db = custody_db::CustodyDb::new(pool.clone());
    db.insert_readiness_transition("scanner.ethereum", "degradation", Some("test reason"))
        .await;
    let transitions = db
        .list_readiness_transitions(None, 10)
        .await
        .expect("list transitions");
    assert!(transitions
        .iter()
        .any(|t| t.feature == "scanner.ethereum"
            && t.kind == "degradation"
            && t.reason.as_deref() == Some("test reason")));

    let silence = db
        .insert_readiness_silence(
            "test.feature.migrations",
            None,
            chrono::Utc::now() + chrono::Duration::hours(1),
            "migration test",
            "test@runner",
        )
        .await
        .expect("insert silence");
    assert_eq!(silence.feature, "test.feature.migrations");
    assert_eq!(silence.reason, "migration test");
    assert!(silence.expires_at > silence.starts_at);

    let active = db.list_active_silences().await.expect("list active");
    assert!(active.iter().any(|s| s.feature == "test.feature.migrations"));

    // Cleanup so re-running the test against the same DB stays idempotent.
    sqlx::query("DELETE FROM readiness_silences WHERE feature = 'test.feature.migrations'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM readiness_transitions WHERE feature = 'scanner.ethereum' AND reason = 'test reason'")
        .execute(&pool)
        .await
        .unwrap();
}
