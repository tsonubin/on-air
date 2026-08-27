use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MOCK_PIN: &str = "123456";

#[derive(Default)]
pub struct PairingState {
    pin: String,
    tokens: HashSet<String>,
}

impl PairingState {
    pub fn new() -> Self {
        PairingState {
            pin: generate_pin(),
            tokens: HashSet::new(),
        }
    }

    pub fn mock() -> Self {
        PairingState {
            pin: MOCK_PIN.to_string(),
            tokens: HashSet::new(),
        }
    }

    pub fn pin(&self) -> &str {
        &self.pin
    }

    pub fn verify(&mut self, pin: &str) -> Option<String> {
        if pin != self.pin {
            return None;
        }
        let token = issue_token();
        self.tokens.insert(token.clone());
        Some(token)
    }

    pub fn token_valid(&self, token: &str) -> bool {
        self.tokens.contains(token)
    }
}

pub fn generate_pin() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:06}", (nanos % 1_000_000) as u32)
}

fn issue_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("onair-{nanos}")
}

pub type SharedPairing = Mutex<PairingState>;
