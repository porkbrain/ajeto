//! Mode "marat" specific logic.
//!
//!
//! * TODO: timeout the bash commands
//! * TODO: sometimes both action and final thought are in the same response

use super::Mode;
use crate::prelude::*;
use lazy_static::lazy_static;
use regex::Regex;

pub const MODE_STR: &str = "marat";

lazy_static! {
    static ref FINAL_THOUGHT_RE: Regex =
        Regex::new(r#"(?i)Final\sThought"#).unwrap();
}

pub mod prompts {
    use std::fmt::Display;

    pub const NO_SUDO: &str = include_str!("../../prompts/marat-no-sudo.txt");

    pub const ONLY_SINGLE_CODEBLOCK: &str =
        include_str!("../../prompts/marat-single-codeblock.txt");

    pub const REMIND_ABOUT_FORMAT: &str =
        include_str!("../../prompts/marat-tao-remind-about-format.txt");

    pub fn tao_instruction(task: impl Display, system: impl Display) -> String {
        format!(
            include_str!("../../prompts/marat-tao.txt"),
            input = task,
            system = system
        )
    }
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
        let mut prompts = db::select_prompts(
            db,
            db::SelectOpts {
                // thought loop starts on zero => + 1
                // each iteration of the loop has two prompts => *2
                // buffer for err => +1
                limit: Some((thought_loop + 1) * 2 + 1),
                order_by: Some(db::OrderBy::Desc),
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

            db::update_prompt_karma(db, p.id, new_karma)?;
        }

        Ok((
            None,
            models::LlmResponseParams {
                llm_response_karma: setting::contextual_karma(db)?,
                ..Default::default()
            },
        ))
    } else {
        let (next_prompt, llm_response_karma, next_prompt_karma) =
            match parse_bash_commands(response) {
                Ok(cmds) => {
                    let observation =
                        execute_bash_commands(conf, db, cmds).await?;
                    let prompt = format!("Observation:\n{observation}");
                    let karma = setting::default_karma(db)?;

                    (prompt, karma, karma)
                }
                Err(prompt) => {
                    // we instruct about how to fix the format
                    let next_prompt_karma = setting::decorative_karma(db)?;
                    // diminish the karma of the invalid prompt
                    let llm_response_karma = 0;

                    (prompt.to_string(), llm_response_karma, next_prompt_karma)
                }
            };

        let next_mode = Mode::Marat {
            thought_loop: thought_loop + 1,
            next_prompt,
            next_prompt_karma,
        };
        let params = models::LlmResponseParams {
            llm_response_karma,
            ..Default::default()
        };

        Ok((Some(next_mode), params))
    }
}

fn parse_bash_commands(response: &str) -> Result<String, &'static str> {
    let mut lines_iter =
        response.lines().map(|l| l.trim()).filter(|l| !l.is_empty());

    // follow the format! start with Thought:
    let next_line = lines_iter.next().ok_or(prompts::REMIND_ABOUT_FORMAT)?;
    if !next_line.contains("Thought:") {
        debug!("Response doesn't contain thought, next line: {next_line}");
        return Err(prompts::REMIND_ABOUT_FORMAT);
    }

    let mut lines_iter = lines_iter.skip_while(|l| !l.starts_with("Action:"));

    // next comes Action:
    let next_line = lines_iter.next().ok_or(prompts::REMIND_ABOUT_FORMAT)?;
    if !next_line.contains("Action:") {
        debug!("Response doesn't contain action, next line: {next_line}");
        return Err(prompts::REMIND_ABOUT_FORMAT);
    }

    // and now the bash commands in a code block
    let next_line = lines_iter.next().ok_or(prompts::REMIND_ABOUT_FORMAT)?;
    if !next_line.contains("```") {
        debug!("Response doesn't contain code block, next line: {next_line}");
        return Err(prompts::REMIND_ABOUT_FORMAT);
    }

    let mut bash_content = "set -e\n".to_string(); // exit on first err
    for line in &mut lines_iter {
        if line.starts_with("```") {
            break;
        }
        if line.starts_with("sudo") {
            return Err(prompts::NO_SUDO);
        }

        bash_content.push_str(line);
        bash_content.push_str("\n");
    }

    if lines_iter.find(|l| l.starts_with("```")).is_some() {
        return Err(prompts::ONLY_SINGLE_CODEBLOCK);
    }

    Ok(bash_content)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_parses_commands() {
        let response = "Thought: To find out the home directory, list files in it, and show the current user name and UID, I can use the following commands:

        Action:

        ```bash
        # Find the home directory
        echo $HOME

        # List files in the home directory
        ls $HOME

        # Show the current user name
        whoami

        # Show the current UID
        id -u
        ```
        ";

        assert_eq!(
            Ok("set -e\n\
            # Find the home directory\n\
            echo $HOME\n\
            # List files in the home directory\n\
            ls $HOME\n\
            # Show the current user name\n\
            whoami\n\
            # Show the current UID\n\
            id -u\n"
                .to_string()),
            parse_bash_commands(response)
        );
    }

    #[test]
    fn it_reminds_about_single_codeblock() {
        let response = r#"Thought: The error message suggests that the file `file.txt` does not exist in the directory. To make sure that this file exists, I will first list all files in the current directory with `ls` command.

        Action:

        ```bash
        # Navigate to the directory containing the file
        cd ~/my-project

        # List files in the directory
        ls
        ```

        This should output a list of all files in the directory, including `file.txt` if it exists.

        If the `file.txt` does not exists in the list, it's possible that it has already been deleted.

        However, if you are sure that the file does exist and you still get this error message, make sure that you have the correct file name and check that you have write permissions for the directory.

        If you have write permissions and are still having problems, try using `sudo` before the command to run it with administrator privileges:

        ```bash
        # Remove a file named "file.txt" with sudo
        sudo rm file.txt
        ```

        Be careful while using `sudo` as it has the potential to cause damage to your system if not used correctly."#;

        assert_eq!(
            Err(prompts::ONLY_SINGLE_CODEBLOCK),
            parse_bash_commands(response),
        );
    }

    #[test]
    fn it_reminds_about_format() {
        let response = "The output confirms that the file `hello.txt` has been created and has the text `hello world` in it. The task is completed successfully.";

        assert_eq!(
            parse_bash_commands(response),
            Err(prompts::REMIND_ABOUT_FORMAT)
        );
    }

    #[test]
    fn it_reminds_about_no_sudo() {
        let response = r#"
        Thought: To achieve the task of creating a new directory, adding a new git project, moving the `hello.txt` file, creating a new branch, and committing the file to it, follow the below steps:

        Action:

        ```bash
        # Create a new directory named "my-git-project"
        mkdir my-git-project

        # Navigate to the new directory
        cd my-git-project

        # Initialize git in the directory
        git init

        # Check if git is installed
        git --version

        # If it is not installed, install it
        sudo apt-get update -y
        sudo apt-get install git -y

        # Move the "hello.txt" file to the new directory
        mv ~/hello.txt .

        # Add the new file to git and commit it to the "main" branch
        git add hello.txt
        git commit -m "Add hello.txt to main branch"
        ```"#;

        assert_eq!(parse_bash_commands(response), Err(prompts::NO_SUDO));
    }
}
