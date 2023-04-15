//! Defines which prompts to include with each LLM request for context.

use crate::prelude::*;
use chrono::Utc;
use std::{collections::HashSet, iter};

pub struct History {
    prompts: Vec<models::Prompt>,
}

impl History {
    pub fn from(db: &DbClient) -> Result<Self> {
        let prompts = select_prompts(
            db,
            SelectOpts {
                limit: Some(setting::recent_history_limit(db)?),
                order_by: Some(OrderBy::Desc),
            },
        )?;

        Ok(Self { prompts })
    }

    pub fn into_llm_prompt(
        self,
        db: &DbClient,
        content: String,
        content_tokens: i32,
    ) -> Result<serde_json::Value> {
        let Self { mut prompts } = self;

        if prompts.len() > 2 {
            let mut total_tokens =
                content_tokens + prompts.iter().map(|p| p.tokens).sum::<i32>();
            let max_tokens = setting::max_tokens(db)?;
            let min_score_to_include_prompt =
                setting::min_score_to_include_prompt(db)?;

            let last_id = *prompts[0].id;
            let now = Utc::now();

            let expiring_id_discount = setting::expiring_id_discount(db)?;
            let expiring_created_at_discount =
                setting::expiring_created_at_discount(db)?;
            let len_discount = setting::len_discount(db)?;
            let karma_multiplier = setting::karma_multiplier(db)?;

            let mut scores = prompts
                .iter()
                .filter_map(|prompt| {
                    let elapsed_minutes = now
                        .signed_duration_since(prompt.created_at())
                        .num_minutes()
                        .max(0);

                    let i = (1.0
                        / ((last_id - *prompt.id).max(1) as f32
                            * expiring_id_discount))
                        .min(1.0);
                    let m = (1.0
                        / (elapsed_minutes as f32
                            * expiring_created_at_discount))
                        .min(1.0);
                    let l = (1.0
                        / (prompt.tokens.max(1) as f32 * len_discount))
                        .min(1.0);
                    let k = prompt.karma as f32 * karma_multiplier;
                    let score = i * m * l * k;

                    // info!("{i:.4} * {m:.4} * {l:.4} * {k:.4} = {score:.4}");

                    if min_score_to_include_prompt > score {
                        total_tokens -= prompt.tokens;
                        None
                    } else {
                        Some((prompt.id, prompt.tokens, score))
                    }
                })
                .collect::<Vec<_>>();
            if total_tokens > max_tokens {
                scores.sort_by(|(_, _, a), (_, _, b)| b.total_cmp(a)); // DESC
            }
            while total_tokens > max_tokens && scores.len() > 1 {
                let (_, tokens, _) = scores.pop().unwrap();
                total_tokens -= tokens;
            }
            let prompts_to_include: HashSet<_> =
                scores.into_iter().map(|(id, _, _)| id).collect();
            prompts.retain(|p| prompts_to_include.contains(&p.id));
        }

        info!(
            "Included history IDs: {:?}",
            prompts.iter().map(|p| p.id).collect::<Vec<_>>()
        );

        Ok(serde_json::Value::Array(
            prompts
                .into_iter()
                .rev()
                .chain(iter::once(models::Prompt {
                    role: "user".to_string(),
                    content,
                    ..Default::default()
                }))
                .map(|p| {
                    let mut prompt = serde_json::Map::new();
                    prompt.insert("role".to_string(), p.role.into());
                    prompt.insert("content".to_string(), p.content.into());
                    serde_json::Value::Object(prompt)
                })
                .collect(),
        ))
    }
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
mod tests {
    use super::*;
    use crate::conf;
    use regex::Regex;

    #[test]
    fn it_inserts_and_selects_prompt() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        conf::migrations().to_latest(&mut db).unwrap();
        let db = db_client(db);

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
            insert_prompt(&db, prompt).unwrap();
        }

        // selects them

        let mut prompts_iter = select_prompts(&db, SelectOpts::default())
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
