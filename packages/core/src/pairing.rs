use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::fmt::Write;
use std::io::{self, Write as IoWrite};
use std::path::PathBuf;
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
    StorageUnavailable,
}

pub struct PairingState {
    pin: String,
    tokens: VecDeque<String>, // SHA-256 digests, never bearer credentials.
    storage_path: Option<PathBuf>,
    failed_attempts: u8,
    blocked_until: Option<Instant>,
    rotate_pin: bool,
}

impl PairingState {
    pub fn new() -> Self {
        PairingState {
            pin: generate_pin(),
            tokens: VecDeque::new(),
            storage_path: None,
            failed_attempts: 0,
            blocked_until: None,
            rotate_pin: true,
        }
    }

    pub fn persistent(path: PathBuf) -> Self {
        let mut state = Self::new();
        match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<VecDeque<String>>(&bytes) {
                Ok(tokens)
                    if tokens.len() <= MAX_PAIRED_TOKENS
                        && tokens.iter().all(|token| {
                            token.len() == 64 && token.bytes().all(|c| c.is_ascii_hexdigit())
                        }) =>
                {
                    state.tokens = tokens
                }
                _ => {
                    eprintln!("Could not read saved remote pairings; a new PIN pairing is required")
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("Could not load saved remote pairings: {error}"),
        }
        state.storage_path = Some(path);
        state
    }

    fn persist(&self) -> io::Result<()> {
        let Some(path) = &self.storage_path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        // Recover an interrupted prior atomic write, without following a symlink.
        match std::fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec(&self.tokens)?)?;
        file.sync_all()?;
        drop(file);
        #[cfg(windows)]
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        std::fs::rename(temporary, path)
    }

    pub fn mock() -> Self {
        PairingState {
            pin: MOCK_PIN.to_string(),
            tokens: VecDeque::new(),
            storage_path: None,
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
        let previous = self.tokens.clone();
        if self.tokens.len() == MAX_PAIRED_TOKENS {
            self.tokens.pop_front();
        }
        self.tokens
            .push_back(format!("{:x}", Sha256::digest(token.as_bytes())));
        if let Err(error) = self.persist() {
            self.tokens = previous;
            eprintln!("Could not save remote pairing: {error}");
            return Err(VerifyError::StorageUnavailable);
        }
        if self.rotate_pin {
            self.pin = generate_pin();
        }
        Ok(token)
    }

    pub fn token_valid(&self, token: &str) -> bool {
        let digest = format!("{:x}", Sha256::digest(token.as_bytes()));
        self.tokens.iter().any(|candidate| candidate == &digest)
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
