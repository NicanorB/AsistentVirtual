use std::sync::{Arc, OnceLock};

use sqlx::{Connection, Executor, PgConnection, PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::{
    build_router,
    common::{AppConfig, AppState},
};

pub mod auth;

static TEST_DATABASE_URL: OnceLock<String> = OnceLock::new();

pub(super) struct TestApp {
    app: axum::Router,
    pool: PgPool,
    database_name: String,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let database_name = self.database_name.clone();
        let pool = self.pool.clone();

        tokio::spawn(async move {
            pool.close().await;
            drop_test_database(&database_name).await;
        });
    }
}

fn test_database_name() -> String {
    format!("asistent_virtual_test_{}", Uuid::new_v4().simple())
}

fn test_database_url_for(database_name: &str) -> String {
    format!(
        "{}/{}",
        test_database_url().trim_end_matches('/'),
        database_name
    )
}

fn admin_database_url() -> String {
    format!("{}/postgres", test_database_url().trim_end_matches('/'))
}

fn test_database_url() -> &'static str {
    TEST_DATABASE_URL.get_or_init(|| {
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests")
    })
}

async fn create_test_pool() -> (PgPool, String) {
    let database_name = test_database_name();
    let mut admin_connection = PgConnection::connect(&admin_database_url())
        .await
        .expect("admin database connection should succeed");

    admin_connection
        .execute(format!(r#"CREATE DATABASE "{}""#, database_name).as_str())
        .await
        .expect("test database creation should succeed");

    let database_url = test_database_url_for(&database_name);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("test database connection should succeed");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("migrations should succeed");

    (pool, database_name)
}

async fn drop_test_database(database_name: &str) {
    let mut admin_connection = PgConnection::connect(&admin_database_url())
        .await
        .expect("admin database connection should succeed");

    admin_connection
        .execute(
            format!(
                r#"DROP DATABASE IF EXISTS "{}" WITH (FORCE)"#,
                database_name
            )
            .as_str(),
        )
        .await
        .expect("test database cleanup should succeed");
}

fn test_config() -> Arc<AppConfig> {
    Arc::new(AppConfig {
        jwt_access_secret: "integration-test-access-secret".to_string(),
        jwt_refresh_secret: "integration-test-refresh-secret".to_string(),
        access_ttl: time::Duration::minutes(5),
        refresh_ttl: time::Duration::days(30),
    })
}

pub(super) async fn test_app() -> TestApp {
    let (pool, database_name) = create_test_pool().await;

    let state = AppState {
        pool: pool.clone(),
        config: test_config(),
    };

    TestApp {
        app: build_router(state),
        pool,
        database_name,
    }
}
