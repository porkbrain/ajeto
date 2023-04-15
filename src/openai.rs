//! Utils for communication with OpenAI models.

use crate::prelude::*;
use reqwest::Client;
use serde_json::json;

/// Given string, estimate how many tokens would that string be represented with
/// in OpenAI universe.
/// We give a pessimistic estimate.
///
///
/// * TODO: use a tokenizer
pub fn estimate_tokens(s: &str) -> i32 {
    s.chars().filter(|c| c.is_whitespace()).count().max(1) as i32 * 2
}

pub async fn respond_to(
    conf: &Conf,
    db: &DbClient,
    params: models::ImmediatePromptParams,
    prompt: serde_json::Value,
) -> Result<String> {
    let chat_endpoint = setting::openai_chat_endpoint(db)?;
    let model = setting::openai_model(db)?;

    let client = Client::new();
    let response = client
        .post(chat_endpoint)
        .header("Authorization", format!("Bearer {}", conf.openai_api_key))
        .json(&json!({
            "model": model,
            "messages": prompt,
            "stop": params.stop,
        }))
        .send()
        .await?;

    let completion: serde_json::Value = response.json().await?;
    let message = completion["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| {
            anyhow!("No response in OpenAI response: {completion:#?}")
        })?
        .to_string();

    Ok(message)
}
