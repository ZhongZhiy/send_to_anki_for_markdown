use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single Anki note payload compatible with Anki-Connect's `addNote(s)` APIs.
#[derive(Serialize)]
pub struct AnkiNote {
    #[serde(rename = "deckName")]
    pub deck_name: String,
    #[serde(rename = "modelName")]
    pub model_name: String,
    pub fields: HashMap<String, String>,
    pub tags: Vec<String>,
}

#[derive(Serialize)]
pub struct AddNotesParams {
    pub notes: Vec<AnkiNote>,
}

/// Standard Anki-Connect request envelope.
#[derive(Serialize)]
pub struct AnkiRequest {
    pub action: String,
    pub version: u8,
    pub params: AddNotesParams,
}

/// Standard Anki-Connect response envelope.
#[derive(Deserialize, Debug)]
pub struct AnkiResponse {
    pub result: Option<Vec<Option<u64>>>,
    pub error: Option<String>,
}

pub async fn add_notes(
    anki_connect_url: &str,
    notes: Vec<AnkiNote>,
    print_json: bool,
    dry_run: bool,
) -> anyhow::Result<Vec<Option<u64>>> {
    let client = Client::new();
    let request = AnkiRequest {
        action: "addNotes".to_string(),
        version: 6,
        params: AddNotesParams { notes },
    };

    if print_json || dry_run {
        println!(
            "Request JSON:\n{}",
            serde_json::to_string_pretty(&request).unwrap()
        );
    }

    if dry_run {
        return Ok(Vec::new());
    }

    let response = client.post(anki_connect_url).json(&request).send().await?;

    let status = response.status();
    let body_text = response.text().await?;
    if !status.is_success() {
        anyhow::bail!("Anki-Connect HTTP error: {} {}", status.as_u16(), status);
    }

    let anki_resp: AnkiResponse = serde_json::from_str(&body_text).map_err(|e| {
        anyhow::anyhow!(
            "Failed to decode Anki-Connect JSON response: {}. Body: {}",
            e,
            body_text
        )
    })?;

    if let Some(err) = anki_resp.error {
        anyhow::bail!("Anki-Connect error: {}", err);
    }

    Ok(anki_resp.result.unwrap_or_default())
}
