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
/// Ceiling on wrong PINs across every peer per rolling window, so rotating
/// source addresses cannot multiply the guess rate.
const GLOBAL_MAX_FAILURES: usize = 10;
const GLOBAL_FAILURE_WINDOW: Duration = Duration::from_secs(60);
/// Wrong PINs since the last rotation after which the PIN is replaced, so the
/// PIN space cannot be swept slowly over time.
const PIN_ROTATION_FAILURES: u32 = 50;
/// A peer's wrong PINs are forgotten after this long without an attempt.
const PEER_FAILURE_MEMORY: Duration = Duration::from_secs(600);

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

impl Attempts {
    fn blocked(&self, now: Instant) -> bool {
        self.blocked_until.is_some_and(|deadline| deadline > now)
    }

    fn failures_remembered(&self, now: Instant) -> bool {
        self.failed > 0 && now.saturating_duration_since(self.last_attempt) < PEER_FAILURE_MEMORY
    }

    /// Blocked or still carrying failures: must never be dropped to make room.
    fn live(&self, now: Instant) -> bool {
        self.blocked(now) || self.failures_remembered(now)
    }
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
    /// Wrong-PIN times across all peers within [`GLOBAL_FAILURE_WINDOW`].
    recent_failures: VecDeque<Instant>,
    failures_since_rotation: u32,
    rotate_pin: bool,
}

impl PairingState {
    pub fn new() -> Self {
        PairingState {
            pin: generate_pin(),
            tokens: VecDeque::new(),
            storage_path: None,
            attempts: HashMap::new(),
            recent_failures: VecDeque::new(),
            failures_since_rotation: 0,
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
            recent_failures: VecDeque::new(),
            failures_since_rotation: 0,
            rotate_pin: false,
        }
    }

    pub fn pin(&self) -> &str {
        &self.pin
    }

    /// The peer's bucket, or `None` when the table is full of live entries:
    /// a blocked or failing peer is never evicted, so the newcomer is refused.
    fn attempts_for(&mut self, peer: Option<IpAddr>, now: Instant) -> Option<&mut Attempts> {
        if !self.attempts.contains_key(&peer) && self.attempts.len() >= MAX_TRACKED_PEERS {
            self.attempts.retain(|_, attempts| attempts.live(now));
            if self.attempts.len() >= MAX_TRACKED_PEERS {
                return None;
            }
        }
        let attempts = self.attempts.entry(peer).or_insert(Attempts {
            failed: 0,
            blocked_until: None,
            last_attempt: now,
        });
        if !attempts.failures_remembered(now) {
            attempts.failed = 0;
        }
        attempts.last_attempt = now;
        Some(attempts)
    }

    fn global_lockout(&mut self, now: Instant) -> bool {
        while self.recent_failures.front().is_some_and(|failed_at| {
            now.saturating_duration_since(*failed_at) >= GLOBAL_FAILURE_WINDOW
        }) {
            self.recent_failures.pop_front();
        }
        self.recent_failures.len() >= GLOBAL_MAX_FAILURES
    }

    fn record_failure(&mut self, now: Instant) {
        self.recent_failures.push_back(now);
        self.failures_since_rotation += 1;
        if self.failures_since_rotation >= PIN_ROTATION_FAILURES {
            self.failures_since_rotation = 0;
            if self.rotate_pin {
                self.pin = generate_pin();
            }
        }
    }

    fn pin_matches(&self, candidate: &str) -> bool {
        candidate.len() == self.pin.len()
            && bool::from(candidate.as_bytes().ct_eq(self.pin.as_bytes()))
    }

    /// Check `pin` for `peer` (global and per-peer lockout, constant-time
    /// compare) and prepare the token. Nothing is persisted or committed yet;
    /// call [`persist_tokens`] with the snapshot, then [`commit`](Self::commit).
    pub fn begin_verify(
        &mut self,
        pin: &str,
        peer: Option<IpAddr>,
    ) -> Result<PreparedPairing, VerifyError> {
        self.begin_verify_at(pin, peer, Instant::now())
    }

    fn begin_verify_at(
        &mut self,
        pin: &str,
        peer: Option<IpAddr>,
        now: Instant,
    ) -> Result<PreparedPairing, VerifyError> {
        if self.global_lockout(now) {
            return Err(VerifyError::RateLimited);
        }
        let matches = self.pin_matches(pin);
        let Some(attempts) = self.attempts_for(peer, now) else {
            return Err(VerifyError::RateLimited);
        };
        if attempts.blocked(now) {
            return Err(VerifyError::RateLimited);
        }
        attempts.blocked_until = None;
        if !matches {
            attempts.failed = attempts.failed.saturating_add(1);
            if attempts.failed >= MAX_FAILED_ATTEMPTS {
                attempts.failed = 0;
                attempts.blocked_until = Some(now + LOCKOUT_DURATION);
            }
            self.record_failure(now);
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
            self.failures_since_rotation = 0;
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

    fn peer(n: u32) -> Option<IpAddr> {
        Some(IpAddr::V4(Ipv4Addr::from(0x0a00_0000 + n)))
    }

    #[test]
    fn rotating_peers_cannot_exceed_the_global_ceiling() {
        let mut pairing = PairingState::mock();
        let now = Instant::now();
        let wrong_guesses = (0..65)
            .filter(|n| {
                pairing.begin_verify_at("000000", peer(*n), now).err()
                    == Some(VerifyError::InvalidPin)
            })
            .count();
        assert_eq!(wrong_guesses, GLOBAL_MAX_FAILURES);
        assert_eq!(
            pairing.begin_verify_at(MOCK_PIN, peer(1000), now).err(),
            Some(VerifyError::RateLimited),
            "every peer is locked out once the global ceiling is hit"
        );
        assert!(
            pairing
                .begin_verify_at(MOCK_PIN, peer(1000), now + GLOBAL_FAILURE_WINDOW)
                .is_ok(),
            "the global lockout lifts once the window passes"
        );
    }

    #[test]
    fn a_full_table_fails_closed_instead_of_evicting_a_blocked_peer() {
        let mut pairing = PairingState::mock();
        let now = Instant::now();
        let attacker = peer(0);
        for _ in 0..MAX_FAILED_ATTEMPTS {
            let _ = pairing.begin_verify_at("000000", attacker, now);
        }
        for n in 1..MAX_TRACKED_PEERS as u32 {
            pairing.attempts.insert(
                peer(n),
                Attempts {
                    failed: 1,
                    blocked_until: None,
                    last_attempt: now,
                },
            );
        }
        assert_eq!(
            pairing.begin_verify_at(MOCK_PIN, peer(999), now).err(),
            Some(VerifyError::RateLimited),
            "a new peer is refused while the table is full of live entries"
        );
        assert_eq!(
            pairing.begin_verify_at(MOCK_PIN, attacker, now).err(),
            Some(VerifyError::RateLimited),
            "the blocked attacker must stay blocked"
        );
    }

    #[test]
    fn pin_rotates_after_cumulative_failures() {
        let mut pairing = PairingState::new();
        let first = pairing.pin().to_string();
        let wrong = if first == "000000" {
            "111111"
        } else {
            "000000"
        };
        let mut now = Instant::now();
        for n in 0..PIN_ROTATION_FAILURES {
            assert_eq!(pairing.pin(), first, "rotated early after {n} failures");
            // Space guesses out so neither lockout interferes.
            now += GLOBAL_FAILURE_WINDOW;
            assert_eq!(
                pairing.begin_verify_at(wrong, peer(n), now).err(),
                Some(VerifyError::InvalidPin)
            );
        }
        assert_ne!(pairing.pin(), first);
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
