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

            let latest_id = *prompts[0].id;
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

                    let score = score(
                        prompt,
                        ScoreEqParams {
                            elapsed_minutes,
                            latest_id,
                            ξ: karma_multiplier,
                            τ: expiring_created_at_discount,
                            ζ: expiring_id_discount,
                            φ: len_discount,
                        },
                    );

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

struct ScoreEqParams {
    /// The age of a prompt in minutes affects the score negatively.
    elapsed_minutes: i64,
    /// The relative distance, in number of prompts between the considered and
    /// the latest one, affects the score negatively.
    latest_id: i32,
    /// Karma points affect prompt score positively, ie. increase its chance to
    /// be included in the next LLM input.
    /// - `ξ` as `karma_multiplier` (eg. 10)
    ξ: f32,
    /// - `τ` as `expiring_created_at_discount` (eg. 300.0)
    τ: f32,
    /// - `ζ` as `expiring_id_discount` (eg. 3.0)
    ζ: f32,
    /// The length of a prompt (in tokens) affects the score negatively.
    /// - `φ` as `len_discount` (eg. 0.01)
    φ: f32,
}

/// The score equation where
/// `n` is the latest prompt,
/// `|i|` is the length of prompt `i`,
/// `k_i` is the karma of prompt `i`,
/// `t_i` are the minutes elapsed since creation of prompt `i`:
///
/// ```text
/// s_i =
///     (ξ k_i)
///     min( 1 , (1 / ( φ |i| )) )
///     min( 1 , (1 / ( τ t_i )) )
///     min( 1 , (1 / ( ζ (n - i) )) )
/// ```
fn score(prompt: &models::Prompt, params: ScoreEqParams) -> f32 {
    let ScoreEqParams {
        elapsed_minutes,
        latest_id,
        ξ,
        τ,
        ζ,
        φ,
    } = params;

    let models::Prompt {
        id, karma, tokens, ..
    } = prompt;

    let i = if ζ >= 0.0 {
        (1.0 / ((latest_id - **id).max(1) as f32 * ζ)).min(1.0)
    } else {
        1.0
    };
    let m = if τ >= 0.0 {
        (1.0 / (elapsed_minutes as f32 * τ)).min(1.0)
    } else {
        1.0
    };
    let l = if φ >= 0.0 {
        (1.0 / ((*tokens).max(1) as f32 * φ)).min(1.0)
    } else {
        1.0
    };
    let k = *karma as f32 * ξ;
    let score = i * m * l * k;

    debug!("{i:.4} * {m:.4} * {l:.4} * {k:.4} = {score:.4}");

    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prop_assert, proptest};

    proptest! {
        #[test]
        fn test_score(
            latest_id in 100i32..500,
            prompt_id in 0i32..100,
            karma in 0i32..100_000,
            tokens in 0i32..10_000,
            elapsed_minutes in 0i64..1_000,
            ξ in 0.0f32..3000.0,
            τ in 0.0f32..3000.0,
            ζ in 0.0f32..3000.0,
            φ in 0.0f32..3000.0,
        ) {
            prop_assert!(prompt_id <= latest_id);

            let score = score(
                &models::Prompt {
                    id: models::PromptId(prompt_id),
                    karma,
                    tokens,
                    ..Default::default()
                },
                ScoreEqParams {
                    elapsed_minutes,
                    latest_id,
                    ξ,
                    τ,
                    ζ,
                    φ,
                },
            );
            prop_assert!(karma as f32 * ξ >= score);
            prop_assert!(score >= 0.0);
        }
    }
}
