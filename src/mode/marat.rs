//! Mode "marat" specific logic.
//!
//!
//! * TODO: timeout the bash commands

use super::Mode;
use crate::{
    history::{select_prompts, update_prompt_karma, OrderBy, SelectOpts},
    prelude::*,
};
use lazy_static::lazy_static;
use regex::Regex;
use std::iter;

pub const MODE_STR: &str = "marat";

lazy_static! {
    static ref FINAL_THOUGHT_RE: Regex =
        Regex::new(r#"(?i)Final\sThought"#).unwrap();
}

/// The mode ends when string "Final Thought:" is received from the LLM.
///
/// # Last iteration
/// When the mode ends, we lower the karma of the prompts that were used in the
/// thought loop.
/// These prompts were useful to achieve the task, but are not themselves
/// interesting going forward.
/// Similarly, we lower the karma of the instructive prompt.
/// Since the task has been achieved, we demote it to a contextual prompt.
/// The final response is marked as contextual as well.
///
/// # Thought loop
/// While the LLM is trying to achieve the goal, we
/// 1. parse bash commands from the response (see [`parse_bash_commands`])
/// 2. execute the commands (see [`execute_bash_commands`])
/// 3. generate an observation based on the output, or print decorative prompt
/// if LLM doesn't follow the expected format.
pub async fn process_llm_response(
    conf: &Conf,
    db: &DbClient,
    response: &str,
    thought_loop: usize,
) -> Result<(Option<Mode>, models::LlmResponseParams)> {
    if FINAL_THOUGHT_RE.is_match(response) {
        let default_karma = setting::default_karma(db)?;
        let decorative_karma = setting::decorative_karma(db)?;
        let contextual_karma = setting::contextual_karma(db)?;
        let instructive_karma = setting::instructive_karma(db)?;
        let mut prompts = select_prompts(
            db,
            SelectOpts {
                // thought loop starts on zero => + 1
                // buffer for err => +1
                // each iteration of the loop has two prompts => *2
                limit: Some((thought_loop + 2) * 2),
                order_by: Some(OrderBy::Desc),
            },
        )?
        .into_iter()
        .take_while(|p| p.mode == MODE_STR);
        while let Some(p) = prompts.next() {
            let new_karma = match p.karma {
                // reached previous marat loop, stop editing that
                k if k == contextual_karma => break,
                // decorative is e.g. a help with format
                k if k == decorative_karma => 0,
                // thoughts, actions and observations
                k if k == default_karma => decorative_karma,
                // the user input, first prompt
                k if k == instructive_karma => contextual_karma,
                // other, unexpected values, diminish
                k => k / 5,
            };

            update_prompt_karma(db, p.id, new_karma)?;
        }

        Ok((
            None,
            models::LlmResponseParams {
                llm_response_karma: setting::contextual_karma(db)?,
                ..Default::default()
            },
        ))
    } else {
        let (next_prompt, karma) =
            if let Some(cmds) = parse_bash_commands(response) {
                let observation = execute_bash_commands(conf, db, cmds).await?;
                let prompt = format!("Observation:\n{observation}");
                let karma = setting::default_karma(db)?;
                (prompt, karma)
            } else {
                let prompt = include_str!("../../prompts/marat-tao-help.txt")
                    .to_string();
                let karma = setting::decorative_karma(db)?;
                (prompt, karma)
            };

        let next_mode = Mode::Marat {
            thought_loop: thought_loop + 1,
            next_prompt,
        };
        let params = models::LlmResponseParams {
            llm_response_karma: karma,
            ..Default::default()
        };

        Ok((Some(next_mode), params))
    }
}

fn parse_bash_commands(response: &str) -> Option<String> {
    let mut lines_iter = response
        .lines()
        .map(|l| l.trim())
        .skip_while(|l| !l.starts_with("Action:") && !l.starts_with("```"));

    let action_line = lines_iter.next()?;
    if action_line.contains("Action:") {
        let bash_line = lines_iter.next()?;
        if !bash_line.contains("```") {
            return None;
        }
    } else if !action_line.contains("```") {
        return None;
    }

    let lines_iter = iter::once("set -e").chain(lines_iter);
    Some(
        lines_iter
            .take_while(|l| !l.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Operates on the user's home directory.
/// Creates dir "marat" if it doesn't exist.
/// Creates a file "marat/exec-{ms}.sh" where {ms} is the current timestamp in
/// milliseconds with the bash content arg.
async fn execute_bash_commands(
    conf: &Conf,
    db: &DbClient,
    bash_content: String,
) -> Result<String> {
    let mut bash_path = conf.user_home_dir.join("marat");
    if !bash_path.is_dir() {
        tokio::fs::create_dir(&bash_path).await?;
    }

    // generate a new file so that we retain the history of what's been executed
    let ms = chrono::Utc::now().timestamp_millis().to_string();
    bash_path.push(format!("exec-{ms}.sh"));
    tokio::fs::write(&bash_path, bash_content).await?;

    let output = tokio::process::Command::new("bash")
        .arg(bash_path)
        .current_dir(&conf.user_home_dir)
        .output()
        .await?;

    let stdout = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // if the output is too long, truncate it
    let max_len = setting::max_bash_output_len_for_response(db)?;
    let stdout = if stdout.len() > max_len {
        format!(
            "{}\n...\nmax log length reached, {} characters truncated",
            &stdout[0..max_len],
            stdout.len() - max_len
        )
    } else {
        stdout
    };

    let observation = if output.status.success() {
        format!("status code: 0, stdout: \n{stdout}")
    } else {
        format!(
            "status code: {}, stderr: \n{stdout}",
            output.status.code().unwrap_or(1)
        )
    };

    Ok(observation)
}
