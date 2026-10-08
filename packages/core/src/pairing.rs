use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::fmt::Write;
use std::io::{self, Write as IoWrite};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use subtle::ConstantTimeEq;

pub const MOCK_PIN: &str = "123456";
const MAX_FAILED_ATTEMPTS: u8 = 5;
const LOCKOUT_DURATION: Duration = Duration::from_secs(30);
const MAX_PAIRED_TOKENS: usize = 16;
/// Bound on the per-peer lockout table so a LAN scanner cannot grow it.
const MAX_TRACKED_PEERS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    InvalidPin,
    RateLimited,
    StorageUnavailable,
}

/// Lockout bookkeeping for one peer. `None` is the shared bucket used when
/// the peer address is unknown (mock core or no `ConnectInfo`).
#[derive(Debug, Clone, Copy)]
struct Attempts {
    failed: u8,
    blocked_until: Option<Instant>,
    last_attempt: Instant,
}

/// A PIN that checked out, waiting for its token digest to be written to
/// disk before it is committed to the in-memory token list.
pub struct PreparedPairing {
    token: String,
    /// The token list as it will be after commit, ready to persist.
    pub tokens: VecDeque<String>,
    pub storage_path: Option<PathBuf>,
}

pub struct PairingState {
    pin: String,
    tokens: VecDeque<String>, // SHA-256 digests, never bearer credentials.
    storage_path: Option<PathBuf>,
    attempts: HashMap<Option<IpAddr>, Attempts>,
    rotate_pin: bool,
}

impl PairingState {
    pub fn new() -> Self {
        PairingState {
            pin: generate_pin(),
            tokens: VecDeque::new(),
            storage_path: None,
            attempts: HashMap::new(),
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

    pub fn mock() -> Self {
        PairingState {
            pin: MOCK_PIN.to_string(),
            tokens: VecDeque::new(),
            storage_path: None,
            attempts: HashMap::new(),
            rotate_pin: false,
        }
    }

    pub fn pin(&self) -> &str {
        &self.pin
    }

    fn attempts_for(&mut self, peer: Option<IpAddr>, now: Instant) -> &mut Attempts {
        if !self.attempts.contains_key(&peer) && self.attempts.len() >= MAX_TRACKED_PEERS {
            self.attempts.retain(|_, attempts| {
                attempts
                    .blocked_until
                    .is_some_and(|deadline| deadline > now)
                    || attempts.failed > 0
            });
            if self.attempts.len() >= MAX_TRACKED_PEERS {
                if let Some(oldest) = self
                    .attempts
                    .iter()
                    .min_by_key(|(_, attempts)| attempts.last_attempt)
                    .map(|(key, _)| *key)
                {
                    self.attempts.remove(&oldest);
                }
            }
        }
        let attempts = self.attempts.entry(peer).or_insert(Attempts {
            failed: 0,
            blocked_until: None,
            last_attempt: now,
        });
        attempts.last_attempt = now;
        attempts
    }

    fn pin_matches(&self, candidate: &str) -> bool {
        candidate.len() == self.pin.len()
            && bool::from(candidate.as_bytes().ct_eq(self.pin.as_bytes()))
    }

    /// Check `pin` for `peer` (per-peer lockout, constant-time compare) and
    /// prepare the token. Nothing is persisted or committed yet; call
    /// [`persist_tokens`] with the snapshot, then [`commit`](Self::commit).
    pub fn begin_verify(
        &mut self,
        pin: &str,
        peer: Option<IpAddr>,
    ) -> Result<PreparedPairing, VerifyError> {
        let now = Instant::now();
        let matches = self.pin_matches(pin);
        let attempts = self.attempts_for(peer, now);
        if attempts
            .blocked_until
            .is_some_and(|deadline| deadline > now)
        {
            return Err(VerifyError::RateLimited);
        }
        attempts.blocked_until = None;
        if !matches {
            attempts.failed = attempts.failed.saturating_add(1);
            if attempts.failed >= MAX_FAILED_ATTEMPTS {
                attempts.failed = 0;
                attempts.blocked_until = Some(now + LOCKOUT_DURATION);
            }
            return Err(VerifyError::InvalidPin);
        }
        self.attempts.remove(&peer);
        let token = issue_token();
        let mut tokens = self.tokens.clone();
        if tokens.len() == MAX_PAIRED_TOKENS {
            tokens.pop_front();
        }
        tokens.push_back(format!("{:x}", Sha256::digest(token.as_bytes())));
        Ok(PreparedPairing {
            token,
            tokens,
            storage_path: self.storage_path.clone(),
        })
    }

    /// Make a prepared pairing live and rotate the PIN. Returns the bearer
    /// token to hand to the client.
    pub fn commit(&mut self, prepared: PreparedPairing) -> String {
        self.tokens = prepared.tokens;
        if self.rotate_pin {
            self.pin = generate_pin();
        }
        prepared.token
    }

    /// Synchronous verify + persist + commit, for callers that are not on an
    /// async runtime. The HTTP handler splits the steps to keep the write
    /// off the request path.
    pub fn verify(&mut self, pin: &str) -> Result<String, VerifyError> {
        let prepared = self.begin_verify(pin, None)?;
        if let Some(path) = prepared.storage_path.as_deref() {
            if let Err(error) = persist_tokens(path, &prepared.tokens) {
                eprintln!("Could not save remote pairing: {error}");
                return Err(VerifyError::StorageUnavailable);
            }
        }
        Ok(self.commit(prepared))
    }

    pub fn token_valid(&self, token: &str) -> bool {
        let digest = format!("{:x}", Sha256::digest(token.as_bytes()));
        self.tokens
            .iter()
            .any(|candidate| bool::from(candidate.as_bytes().ct_eq(digest.as_bytes())))
    }
}

/// Atomically write the token digests to `path` (0600 on Unix).
pub fn persist_tokens(path: &Path, tokens: &VecDeque<String>) -> io::Result<()> {
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
    file.write_all(&serde_json::to_vec(tokens)?)?;
    file.sync_all()?;
    drop(file);
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(temporary, path)
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

impl Default for PairingState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn lockout_is_per_peer() {
        let mut pairing = PairingState::mock();
        let attacker = Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 66)));
        let phone = Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)));
        for _ in 0..MAX_FAILED_ATTEMPTS {
            assert_eq!(
                pairing.begin_verify("000000", attacker).err(),
                Some(VerifyError::InvalidPin)
            );
        }
        assert_eq!(
            pairing.begin_verify(MOCK_PIN, attacker).err(),
            Some(VerifyError::RateLimited)
        );
        assert!(
            pairing.begin_verify(MOCK_PIN, phone).is_ok(),
            "another peer must not inherit the attacker's lockout"
        );
    }

    #[test]
    fn peer_table_is_bounded() {
        let mut pairing = PairingState::mock();
        for n in 0..(MAX_TRACKED_PEERS as u32 * 2) {
            let peer = Some(IpAddr::V4(Ipv4Addr::from(0x0a00_0000 + n)));
            let _ = pairing.begin_verify("000000", peer);
        }
        assert!(pairing.attempts.len() <= MAX_TRACKED_PEERS);
    }

    #[test]
    fn commit_applies_the_prepared_tokens_and_rotates() {
        let mut pairing = PairingState::new();
        let pin = pairing.pin().to_string();
        let prepared = pairing.begin_verify(&pin, None).unwrap();
        assert!(pairing.tokens.is_empty(), "nothing is live before commit");
        let token = pairing.commit(prepared);
        assert!(pairing.token_valid(&token));
        assert_ne!(pairing.pin(), pin);
    }
}
