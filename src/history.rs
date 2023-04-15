//! Defines which prompts to include with each LLM request for context.

use crate::prelude::*;
use chrono::Utc;
use std::{collections::HashSet, iter};

pub struct History {
    prompts: Vec<models::Prompt>,
}

impl History {
    pub fn from(db: &DbClient) -> Result<Self> {
        let prompts = db::select_prompts(
            db,
            db::SelectOpts {
                limit: Some(setting::recent_history_limit(db)?),
                order_by: Some(db::OrderBy::Desc),
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

                    debug!("{i:.4} * {m:.4} * {l:.4} * {k:.4} = {score:.4}");

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

        debug!(
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
