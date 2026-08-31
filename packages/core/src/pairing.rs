use std::collections::VecDeque;
use std::fmt::Write;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const MOCK_PIN: &str = "123456";
const MAX_FAILED_ATTEMPTS: u8 = 5;
const LOCKOUT_DURATION: Duration = Duration::from_secs(30);
const MAX_PAIRED_TOKENS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    InvalidPin,
    RateLimited,
}

pub struct PairingState {
    pin: String,
    tokens: VecDeque<String>,
    failed_attempts: u8,
    blocked_until: Option<Instant>,
    rotate_pin: bool,
}

impl PairingState {
    pub fn new() -> Self {
        PairingState {
            pin: generate_pin(),
            tokens: VecDeque::new(),
            failed_attempts: 0,
            blocked_until: None,
            rotate_pin: true,
        }
    }

    pub fn mock() -> Self {
        PairingState {
            pin: MOCK_PIN.to_string(),
            tokens: VecDeque::new(),
            failed_attempts: 0,
            blocked_until: None,
            rotate_pin: false,
        }
    }

    pub fn pin(&self) -> &str {
        &self.pin
    }

    pub fn verify(&mut self, pin: &str) -> Result<String, VerifyError> {
        let now = Instant::now();
        if self.blocked_until.is_some_and(|deadline| deadline > now) {
            return Err(VerifyError::RateLimited);
        }
        self.blocked_until = None;

        if pin != self.pin {
            self.failed_attempts = self.failed_attempts.saturating_add(1);
            if self.failed_attempts >= MAX_FAILED_ATTEMPTS {
                self.failed_attempts = 0;
                self.blocked_until = Some(now + LOCKOUT_DURATION);
            }
            return Err(VerifyError::InvalidPin);
        }
        self.failed_attempts = 0;
        let token = issue_token();
        if self.tokens.len() == MAX_PAIRED_TOKENS {
            self.tokens.pop_front();
        }
        self.tokens.push_back(token.clone());
        if self.rotate_pin {
            self.pin = generate_pin();
        }
        Ok(token)
    }

    pub fn token_valid(&self, token: &str) -> bool {
        self.tokens.iter().any(|candidate| candidate == token)
    }
}

pub fn generate_pin() -> String {
    let mut bytes = [0_u8; 4];
    getrandom::fill(&mut bytes).expect("operating-system randomness for pairing PIN");
    format!("{:06}", u32::from_le_bytes(bytes) % 1_000_000)
}

fn issue_token() -> String {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).expect("operating-system randomness for pairing token");
    let mut token = String::with_capacity(6 + bytes.len() * 2);
    token.push_str("onair-");
    for byte in bytes {
        write!(&mut token, "{byte:02x}").expect("writing to a String cannot fail");
    }
    token
}

pub type SharedPairing = Mutex<PairingState>;

impl Default for PairingState {
    fn default() -> Self {
        Self::new()
    }
}
