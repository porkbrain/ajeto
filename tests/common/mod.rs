use std::time::Duration;

use anyhow::{anyhow, Result};
use reqwest::blocking::Client;

pub fn send_request(input: &str) -> Result<()> {
    let client = Client::new();
    let url = "http://localhost:8080/v1";
    let body = serde_json::json!({ "input": input });

    let res = client
        .post(url)
        .timeout(Duration::from_secs(300))
        .json(&body)
        .send()?;
    if res.status().is_success() {
        Ok(())
    } else {
        Err(anyhow!("Request failed: {}", res.status()))
    }
}
