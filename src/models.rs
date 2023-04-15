//! Structs used across all modules.

use chrono::{DateTime, TimeZone, Utc};
use std::ops::Deref;

#[derive(Hash, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct PromptId(pub i32);

#[derive(Default, Debug)]
pub struct Prompt {
    pub id: PromptId,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub mode: String,
    pub karma: i32,
    pub tokens: i32,
}

#[derive(Debug)]
pub struct NewPrompt<'a> {
    pub role: &'a str,
    pub content: &'a str,
    pub mode: &'a str,
    pub tokens: i32,
    pub karma: i32,
}

#[derive(Default, Debug)]
pub struct ImmediatePromptParams {
    /// What karma should be the user prompt inserted into db with
    pub user_prompt_karma: i32,
    /// Up to 4 sequences where the API stops generating further tokens.
    pub stop: Vec<String>,
}

#[derive(Default, Debug)]
pub struct LlmResponseParams {
    /// What karma should be the LLM response inserted into db with
    pub llm_response_karma: i32,
}

impl Prompt {
    pub fn created_at(&self) -> DateTime<Utc> {
        Utc.datetime_from_str(&self.created_at, "%Y-%m-%d %H:%M:%S")
            .expect("Failed to parse date string")
    }
}

impl Deref for PromptId {
    type Target = i32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
