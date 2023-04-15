//! Mode determines decorators for prompt and LLM response interpretation.
//!
//! A keyword which changes the mode is called a directive.
//! A good directive is
//!
//! - short because you say it often;
//! - unambiguously pronounced with accents;
//! - mnemonic for flatter learning curve;
//! - obscure to lower collisions.
//!
//! # Life-cycle
//!
//! 1. We process user input.
//! This determines which mode to use.
//! 2. We generate new LLM prompt.
//! The mode helps with decorators.
//! User input is instructive.
//! 3. We process LLM response.
//! Determines further behavior, such as whether to continue and under which
//! mode, etc.
//!
//!
//! * TODO: escape mode directives

/// Mode "marat" specific logic.
mod marat;

use crate::prelude::*;
use chrono::Utc;
use lazy_static::lazy_static;
use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    /// For when I want a free-style mode, I use directive _"dali"_.
    /// This mode is the most creative.
    /// The prompt is the chat history and the instruction, without any system
    /// decorators.
    ///
    /// # What do I want from this mode?
    ///
    /// **I** Summarize the commit messages in two sentences.
    ///
    /// **O** Human friendly summary.
    Dali { prompt: String },
    /// For when I want it to `$ bash`, I use _"marat"_ as the keyword that
    /// changes the mode.
    ///
    /// # What do I want from this mode?
    ///
    /// **I** Show me current dir and list files
    ///
    /// **O** It runs `pwd` and `ls` and shows me the output.
    ///
    /// **I** Go up one dir level. Next change to the `example` dir. Next make
    /// sure that we're on the develop branch. Next log the past 10 commits
    /// into stdout.
    ///
    /// **O** Prints the last 10 commit messages.
    ///
    /// **I** Git add `example.rs`. Next commit with a relevant message.
    Marat {
        /// What to append to the chat messages next.
        next_prompt: String,
        /// Useful to know because the first prompt should have higher karma.
        ///
        /// A higher karma means that the first prompt is going to be included
        /// as context longer.
        thought_loop: usize,
    },
    /// For when I want it to code, I use directive _"cobol"_.
    Cobol,
    /// For when I want to be explicit about history, I use directive
    /// _"annus"_.
    Annus,
}

lazy_static! {
    static ref MARAT_RE: Regex =
        Regex::new(r#"(\s|\n|^)(?i)marat(\s|$|\W)"#).unwrap();
    static ref DALI_RE: Regex =
        Regex::new(r#"(\s|\n|^)(?i)dali(\s|$|\W)"#).unwrap();
    static ref COBOL_RE: Regex =
        Regex::new(r#"(\s|\n|^)(?i)cobol(\s|$|\W)"#).unwrap();
    static ref ANNUS_RE: Regex =
        Regex::new(r#"(\s|\n|^)(?i)annus(\s|$|\W)"#).unwrap();
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Marat { .. } => marat::MODE_STR,
            Self::Dali { .. } => "dali",
            Self::Annus => "annus",
            Self::Cobol => "cobol",
        }
    }

    /// 1.
    pub fn process_input(db: &DbClient, input: String) -> Result<Self> {
        let input = input.trim();

        if MARAT_RE.shortest_match(&input).is_some() {
            let task = MARAT_RE.replace_all(&input, " ").to_string();
            return Ok(Self::Marat {
                thought_loop: 0,
                next_prompt: format!(
                    include_str!("../prompts/marat-tao.txt"),
                    input = task,
                    system = setting::system(db)?
                ),
            });
        } else if COBOL_RE.shortest_match(&input).is_some() {
            todo!("cobol mode not implemented");
            return Ok(Self::Cobol);
        } else if ANNUS_RE.shortest_match(&input).is_some() {
            todo!("annus mode not implemented");
            return Ok(Self::Annus);
        } else if DALI_RE.shortest_match(&input).is_some() {
            let prompt = DALI_RE.replace_all(&input, " ").to_string();
            return Ok(Self::Dali { prompt });
        } else if let Ok(latest_prompt) = db::latest_prompt(db) {
            let elapsed_seconds = Utc::now()
                .signed_duration_since(latest_prompt.created_at())
                .num_seconds()
                .max(0);
            // if session hasn't ended yet
            if elapsed_seconds < setting::end_session_after_idle_secs(db)? {
                match latest_prompt.mode.as_ref() {
                    "dali" => {
                        return Ok(Self::Dali {
                            prompt: input.to_string(),
                        })
                    }
                    marat::MODE_STR => {
                        return Ok(Self::Marat {
                            thought_loop: 0,
                            next_prompt: format!(
                                include_str!("../prompts/marat-tao.txt"),
                                input = input,
                                system = setting::system(db)?
                            ),
                        });
                    }
                    directive => warn!(
                        "Latest prompt used directive {directive}, \
                    don't know what to do with that"
                    ),
                }
            }
        };

        match setting::default_prompt(db)?.as_str() {
            "dali" => Ok(Self::Dali {
                prompt: input.to_string(),
            }),
            directive => {
                error!("Cannot use {directive} as default prompt");
                Err(anyhow!("Cannot use {directive} as default prompt"))
            }
        }
    }

    /// 2.
    pub fn generate_immediate_prompt(
        &self,
        db: &DbClient,
    ) -> Result<(String, models::ImmediatePromptParams)> {
        match self {
            Mode::Marat {
                next_prompt,
                thought_loop,
            } => Ok((
                next_prompt.clone(),
                models::ImmediatePromptParams {
                    stop: vec!["Observation:".to_string()],
                    user_prompt_karma: if *thought_loop == 0 {
                        setting::instructive_karma(db)?
                    } else {
                        setting::default_karma(db)?
                    },
                    ..Default::default()
                },
            )),
            Mode::Dali { prompt } => Ok((
                prompt.clone(),
                models::ImmediatePromptParams {
                    user_prompt_karma: setting::default_karma(db)?,
                    ..Default::default()
                },
            )),
            _ => todo!("generating prompt not implemented for this mode yet"),
        }
    }

    /// 3.
    pub async fn process_llm_response(
        self,
        conf: &Conf,
        db: &DbClient,
        response: &str,
    ) -> Result<(Option<Mode>, models::LlmResponseParams)> {
        match self {
            Self::Marat { thought_loop, .. } => {
                marat::process_llm_response(conf, db, response, thought_loop)
                    .await
            }
            Self::Dali { .. } => Ok((None, Default::default())),
            _ => todo!("cannot process response from this mode yet"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_constructs_marat() {
        let db = db::client();

        let tests = vec![
            (" there's \nmarat a rainbow", "there's  a rainbow"),
            ("\n   marat a rainbow", " a rainbow"),
            (
                "ok marat heremarat no maratok marat.",
                "ok heremarat no maratok ",
            ),
            ("MaRat", " "),
        ];

        for (i, o) in tests {
            assert_eq!(
                Mode::process_input(&db, i.to_string()).unwrap(),
                Mode::Marat {
                    next_prompt: format!(
                        include_str!("../prompts/marat-tao.txt"),
                        input = o,
                        system = setting::system(&db).unwrap()
                    ),
                    thought_loop: 0,
                }
            );
        }
    }

    #[test]
    fn it_constructs_dali() {
        let db = db::client();

        let tests = vec![
            (" there's \ndali a rainbow", "there's  a rainbow"),
            ("\n   dali a rainbow", " a rainbow"),
            ("ok dali heredali no daliok dali", "ok heredali no daliok "),
            ("DaLi", " "),
        ];

        for (i, o) in tests {
            assert_eq!(
                Mode::process_input(&db, i.to_string()).unwrap(),
                Mode::Dali {
                    prompt: o.to_string()
                }
            );
        }
    }
}
