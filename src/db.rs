//! All things database

pub use rusqlite::Connection as DbConn;

use crate::prelude::*;
use rusqlite_migration::{Migrations, M};
use std::sync::{Arc, Mutex};

pub type DbClient = Arc<Mutex<DbConn>>;

pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(include_str!("../migrations/0001.up.sql"))])
}

pub fn insert_prompt(db: &DbClient, prompt: models::NewPrompt) -> Result<()> {
    let db = db.lock().unwrap();
    db.execute(
        "INSERT INTO prompts (mode, role, content, karma, tokens) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![prompt.mode, prompt.role, prompt.content, prompt.karma, prompt.tokens],
    )?;
    Ok(())
}

pub fn latest_prompt(db: &DbClient) -> Result<models::Prompt> {
    select_prompts(
        db,
        SelectOpts {
            limit: Some(1),
            order_by: Some(OrderBy::Desc),
            ..Default::default()
        },
    )?
    .into_iter()
    .next()
    .ok_or_else(|| anyhow!("No prompts found"))
}

pub fn update_prompt_karma(
    db: &DbClient,
    prompt_id: models::PromptId,
    new_karma: i32,
) -> Result<()> {
    let db = db.lock().unwrap();
    db.execute(
        "UPDATE prompts SET karma = ?2 WHERE id = ?1;",
        rusqlite::params![*prompt_id, new_karma],
    )?;
    Ok(())
}

pub enum OrderBy {
    #[allow(dead_code)]
    Asc,
    Desc,
}

#[derive(Default)]
pub struct SelectOpts {
    pub limit: Option<usize>,
    pub order_by: Option<OrderBy>,
}

pub fn select_prompts(
    db: &DbClient,
    opts: SelectOpts,
) -> Result<Vec<models::Prompt>> {
    let mut sql =
        "SELECT id, mode, role, content, karma, tokens, created_at FROM prompts"
            .to_string();

    if let Some(order_by) = opts.order_by {
        sql.push_str(" ORDER BY id ");
        sql.push_str(match order_by {
            OrderBy::Asc => "ASC",
            OrderBy::Desc => "DESC",
        });
    }

    if let Some(limit) = opts.limit {
        sql.push_str(" LIMIT ");
        sql.push_str(&limit.to_string());
    }

    db.lock()
        .unwrap()
        .prepare(&sql)?
        .query_map([], |row| {
            Ok(models::Prompt {
                id: models::PromptId(row.get(0)?),
                mode: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                karma: row.get(4)?,
                tokens: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .try_fold::<_, _, Result<Vec<models::Prompt>>>(vec![], |mut acc, p| {
            acc.push(p?);
            Ok(acc)
        })
}

#[cfg(test)]
pub fn client() -> DbClient {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    migrations().to_latest(&mut db).unwrap();
    Arc::new(Mutex::new(db))
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn migrations_test() {
        assert!(migrations().validate().is_ok());
    }

    #[test]
    fn it_inserts_and_selects_prompt() {
        let db = db::client();

        // inserts them

        let prompts = vec![
            models::NewPrompt {
                karma: 100,
                tokens: 1000,
                mode: "mode1",
                role: "assistant",
                content: "how can I help?",
            },
            models::NewPrompt {
                karma: 200,
                tokens: 2000,
                mode: "mode2",
                role: "user",
                content: "what's your fav number?",
            },
            models::NewPrompt {
                karma: 300,
                tokens: 3000,
                mode: "mode3",
                role: "hardy",
                content: "1729",
            },
        ];
        for prompt in prompts {
            db::insert_prompt(&db, prompt).unwrap();
        }

        // selects them

        let mut prompts_iter =
            db::select_prompts(&db, db::SelectOpts::default())
                .unwrap()
                .into_iter();

        let prompt = prompts_iter.next().unwrap();
        let first_id = *prompt.id;
        assert_eq!(prompt.role, "assistant");
        assert_eq!(prompt.content, "how can I help?");
        assert_eq!(prompt.karma, 100);
        assert_eq!(prompt.tokens, 1000);
        assert_eq!(prompt.mode, "mode1");

        let prompt = prompts_iter.next().unwrap();
        assert_eq!(*prompt.id, first_id + 1);
        assert_eq!(prompt.role, "user");
        assert_eq!(prompt.content, "what's your fav number?");
        assert_eq!(prompt.karma, 200);
        assert_eq!(prompt.tokens, 2000);
        assert_eq!(prompt.mode, "mode2");

        let prompt = prompts_iter.next().unwrap();
        assert_eq!(*prompt.id, first_id + 2);
        assert_eq!(prompt.role, "hardy");
        assert_eq!(prompt.content, "1729");
        assert_eq!(prompt.karma, 300);
        assert_eq!(prompt.tokens, 3000);
        assert_eq!(prompt.mode, "mode3");

        let date_regex =
            Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$").unwrap();
        assert!(date_regex.is_match(prompt.created_at.as_str()));
    }
}
