/// Deserializes `Conf` object from ENV variables.
mod conf;
/// Defines which prompts to include with each LLM request for context.
mod history;
/// Mode determines decorators for prompt and LLM response interpretation.
mod mode;
/// Structs used across all modules.
pub mod models;
/// Utils for communication with OpenAI models.
mod openai;
/// Common imports
mod prelude;

use crate::history::History;
use mode::Mode;
use prelude::*;
use serde_derive::{Deserialize, Serialize};
use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
};
use warp::Filter;

/// Definition of body for `POST /v1`
#[derive(Serialize, Deserialize)]
struct JsonBody {
    input: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    env_logger::builder().format_timestamp(None).init();

    let conf = Conf::from_env()?;
    let http_addr = conf.http_addr;
    let db = Arc::new(Mutex::new(conf.db()?));

    let recv_route = warp::path!("v1")
        .and(warp::post())
        .and(with_object(conf))
        .and(with_object(db))
        .and(warp::body::json())
        .and_then(move |conf, db: DbClient, cmd| async move {
            let JsonBody { input } = cmd;
            if let Err(e) = recv(conf, db, input.clone()).await {
                error!("Cannot process input:\n\n{input}\n\n---\nError: {e}");
                #[derive(Debug)]
                struct InternalServerError;
                impl warp::reject::Reject for InternalServerError {}
                Err(warp::reject::custom(InternalServerError))
            } else {
                Ok(warp::reply())
            }
        });
    let routes = recv_route.with(warp::cors().allow_any_origin());

    warp::serve(routes).run(http_addr).await;
    Err(anyhow!("Server exited"))
}

/// Clones the object and let's the filter use it.
fn with_object<T: Clone + Send>(
    obj: T,
) -> impl Filter<Extract = (T,), Error = Infallible> + Clone {
    warp::any().map(move || obj.clone())
}

/// Gets user input.
/// Here's a simplified generic flow.
/// Note that the [`Mode`] ultimately decides how the program behaves.
///
/// 1. Processes the input and decides on the [`Mode`] to use
/// 2. Fetches prompt history from db
/// 3. Generates the next prompt to be sent to LLM
/// 4. Stores the next prompt to db history
/// 5. Generates LLM input (history + next prompt)
/// 6. Sends the input to LLM
/// 7. Processes the response
/// 8. Stores the response to db history
/// 9. Decides what to do next based on the output of 7., either goto 2. or stop
///
///
/// * TODO: prompt buffering
async fn recv(conf: Conf, db: DbClient, input: String) -> Result<()> {
    let input = input.trim().to_string();
    if input.is_empty() {
        warn!("Received an empty input");
        return Ok(());
    }

    // 1.
    let mut mode = Mode::process_input(&db, input)?;

    let max_thought_loops = setting::max_thought_loops(&db)?;
    for _ in 0..max_thought_loops {
        // 2.
        let history = History::from(&db)?;

        // 3.
        let (immediate_prompt, params) = mode.generate_immediate_prompt(&db)?;
        let immediate_prompt_tokens =
            openai::estimate_tokens(&immediate_prompt);

        // 4.
        history::insert_prompt(
            &db,
            models::NewPrompt {
                karma: params.user_prompt_karma,
                mode: mode.as_str(),
                role: "user",
                tokens: immediate_prompt_tokens,
                content: &immediate_prompt,
            },
        )?;

        // 5.
        let llm_prompt = history.into_llm_prompt(
            &db,
            immediate_prompt,
            immediate_prompt_tokens,
        )?;

        // 6.
        let response =
            openai::respond_to(&conf, &db, params, llm_prompt).await?;

        // 7.
        let current_mode = mode.as_str();
        let (next_mode, params) =
            mode.process_llm_response(&conf, &db, &response).await?;

        // 8.
        history::insert_prompt(
            &db,
            models::NewPrompt {
                karma: params.llm_response_karma,
                mode: current_mode,
                tokens: openai::estimate_tokens(&response),
                role: "assistant",
                content: &response,
            },
        )?;

        // 9.
        if let Some(next_mode) = next_mode {
            mode = next_mode;
        } else {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {}
