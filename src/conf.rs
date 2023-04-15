//! Deserializes `Conf` object from ENV variables.

use crate::prelude::*;
use serde_derive::Deserialize;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Deserialize, Debug, Clone)]
pub struct Conf {
    pub openai_api_key: String,

    // these have defaults
    #[serde(default = "defaults::sql_conn")]
    pub sql_conn: String,
    #[serde(default = "defaults::http_addr")]
    pub http_addr: SocketAddr,
    #[serde(default = "defaults::user_home_dir")]
    pub user_home_dir: PathBuf,
}

mod defaults {
    use std::{net::SocketAddr, path::PathBuf};

    pub fn sql_conn() -> String {
        ":memory:".to_string()
    }

    pub fn http_addr() -> SocketAddr {
        ([127, 0, 0, 1], 8080).into()
    }

    pub fn user_home_dir() -> PathBuf {
        "/home/ajeto".into()
    }
}

impl Conf {
    pub fn from_env() -> Result<Self> {
        let cfg = config::Config::builder()
            .add_source(config::Environment::default())
            .build()?;

        Ok(cfg.try_deserialize()?)
    }

    pub fn db(&self) -> Result<DbClient> {
        let mut conn = DbConn::open(&self.sql_conn)?;
        db::migrations().to_latest(&mut conn)?;

        Ok(Arc::new(Mutex::new(conn)))
    }
}

pub mod setting {
    use super::*;

    pub fn openai_chat_endpoint(db: &DbClient) -> Result<String> {
        fetch(db, "openai_chat_endpoint")
    }

    pub fn max_thought_loops(db: &DbClient) -> Result<usize> {
        Ok(fetch(db, "max_thought_loops")?.parse()?)
    }

    pub fn openai_model(db: &DbClient) -> Result<String> {
        fetch(db, "openai_model")
    }

    pub fn default_prompt(db: &DbClient) -> Result<String> {
        fetch(db, "default_prompt")
    }

    pub fn system(db: &DbClient) -> Result<String> {
        fetch(db, "system")
    }

    pub fn recent_history_limit(db: &DbClient) -> Result<usize> {
        Ok(fetch(db, "recent_history_limit")?.parse()?)
    }

    pub fn max_tokens(db: &DbClient) -> Result<i32> {
        Ok(fetch(db, "max_tokens")?.parse()?)
    }

    pub fn min_score_to_include_prompt(db: &DbClient) -> Result<f32> {
        Ok(fetch(db, "min_score_to_include_prompt")?.parse()?)
    }

    pub fn default_karma(db: &DbClient) -> Result<i32> {
        Ok(fetch(db, "default_karma")?.parse()?)
    }

    pub fn instructive_karma(db: &DbClient) -> Result<i32> {
        Ok(fetch(db, "instructive_karma")?.parse()?)
    }

    pub fn decorative_karma(db: &DbClient) -> Result<i32> {
        Ok(fetch(db, "decorative_karma")?.parse()?)
    }

    pub fn contextual_karma(db: &DbClient) -> Result<i32> {
        Ok(fetch(db, "contextual_karma")?.parse()?)
    }

    pub fn end_session_after_idle_secs(db: &DbClient) -> Result<i64> {
        Ok(fetch(db, "end_session_after_idle_secs")?.parse()?)
    }

    pub fn expiring_id_discount(db: &DbClient) -> Result<f32> {
        Ok(fetch(db, "expiring_id_discount")?.parse()?)
    }

    pub fn expiring_created_at_discount(db: &DbClient) -> Result<f32> {
        Ok(fetch(db, "expiring_created_at_discount")?.parse()?)
    }

    pub fn len_discount(db: &DbClient) -> Result<f32> {
        Ok(fetch(db, "len_discount")?.parse()?)
    }

    pub fn karma_multiplier(db: &DbClient) -> Result<f32> {
        Ok(fetch(db, "karma_multiplier")?.parse()?)
    }

    pub fn max_bash_output_len_for_response(db: &DbClient) -> Result<usize> {
        Ok(fetch(db, "max_bash_output_len_for_response")?.parse()?)
    }

    fn fetch(db: &DbClient, name: &str) -> Result<String> {
        let db = db.lock().unwrap();

        let mut stmt =
            db.prepare("SELECT value FROM settings WHERE name = ?1")?;
        let value =
            stmt.query_row(rusqlite::params![name], |row| row.get(0))?;

        Ok(value)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn it_loads_settings_ok() -> Result<()> {
            let db = db::client();

            let openai_chat_endpoint = setting::openai_chat_endpoint(&db)?;
            let max_thought_loops = setting::max_thought_loops(&db)?;
            let openai_model = setting::openai_model(&db)?;
            let default_prompt = setting::default_prompt(&db)?;
            let recent_history_limit = setting::recent_history_limit(&db)?;
            let max_tokens = setting::max_tokens(&db)?;
            let min_score_to_include_prompt =
                setting::min_score_to_include_prompt(&db)?;
            let default_karma = setting::default_karma(&db)?;
            let instructive_karma = setting::instructive_karma(&db)?;
            let contextual_karma = setting::contextual_karma(&db)?;
            let _expiring_id_discount = setting::expiring_id_discount(&db)?;
            let _expiring_created_at_discount =
                setting::expiring_created_at_discount(&db)?;
            let _len_discount = setting::len_discount(&db)?;
            let _karma_multiplier = setting::karma_multiplier(&db)?;

            assert_eq!(
                openai_chat_endpoint.to_string(),
                setting::fetch(&db, "openai_chat_endpoint")?
            );
            assert_eq!(
                max_thought_loops.to_string(),
                setting::fetch(&db, "max_thought_loops")?
            );
            assert_eq!(
                openai_model.to_string(),
                setting::fetch(&db, "openai_model")?
            );
            assert_eq!(
                default_prompt.to_string(),
                setting::fetch(&db, "default_prompt")?
            );
            assert_eq!(
                recent_history_limit.to_string(),
                setting::fetch(&db, "recent_history_limit")?
            );
            assert_eq!(
                max_tokens.to_string(),
                setting::fetch(&db, "max_tokens")?
            );
            assert_eq!(
                min_score_to_include_prompt.to_string(),
                setting::fetch(&db, "min_score_to_include_prompt")?
            );
            assert_eq!(
                default_karma.to_string(),
                setting::fetch(&db, "default_karma")?
            );
            assert_eq!(
                instructive_karma.to_string(),
                setting::fetch(&db, "instructive_karma")?
            );
            assert_eq!(
                contextual_karma.to_string(),
                setting::fetch(&db, "contextual_karma")?
            );

            Ok(())
        }
    }
}
