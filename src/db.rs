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

    #[test]
    fn migrations_test() {
        assert!(migrations().validate().is_ok());
    }
}
