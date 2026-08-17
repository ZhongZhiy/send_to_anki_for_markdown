use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const ANKI_CONNECT_URL: &str = "http://localhost:8765";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkiNote {
    #[serde(rename = "deckName")]
    pub deck_name: String,
    #[serde(rename = "modelName")]
    pub model_name: String,
    pub fields: HashMap<String, String>,
    pub tags: Vec<String>,
}

pub fn can_connect() -> bool {
    let payload = serde_json::json!({
        "action": "version",
        "version": 6
    });
    reqwest::blocking::Client::new()
        .post(ANKI_CONNECT_URL)
        .json(&payload)
        .send()
        .is_ok()
}

pub fn create_deck(deck_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::json!({
        "action": "createDeck",
        "version": 6,
        "params": {
            "deck": deck_name
        }
    });
    reqwest::blocking::Client::new()
        .post(ANKI_CONNECT_URL)
        .json(&payload)
        .send()?;
    Ok(())
}

pub fn add_note(note: &AnkiNote) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::json!({
        "action": "addNote",
        "version": 6,
        "params": {
            "note": note
        }
    });
    reqwest::blocking::Client::new()
        .post(ANKI_CONNECT_URL)
        .json(&payload)
        .send()?;
    Ok(())
}
