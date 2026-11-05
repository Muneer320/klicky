//! Windows-only control messages. Decoding is not peer authentication or an
//! acknowledgment of process exit. The transport must verify the pipe's server
//! process and apply one deadline across connection, I/O and command completion.
use std::fmt;

use serde::{Deserialize, Serialize};

use super::paths::validate_pack_name;

pub const PROTOCOL_MAJOR: u32 = 1;
/// Payload limits exclude the single terminating newline.
pub const MAX_REQUEST_BYTES: usize = 4096;
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_ERROR_BYTES: usize = 512;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    // Empty struct variants enforce unknown-field rejection in Serde's tagged form.
    Stop {},
    /// Installer-only stop with an atomic account of the temporary user pause.
    PrepareUpdate {},
    Switch {
        pack: String,
    },
    Volume {
        level: f32,
    },
    Status {},
    Pause {
        paused: bool,
    },
    RetryAudio {},
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: u32,
    pub request_id: u64,
    pub command: Command,
}

impl Request {
    pub fn new(request_id: u64, command: Command) -> Self {
        Self {
            protocol: PROTOCOL_MAJOR,
            request_id,
            command,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        encode(self, MAX_REQUEST_BYTES)
    }

    pub fn decode(frame: &[u8]) -> Result<Self, ProtocolError> {
        let payload = payload(frame, MAX_REQUEST_BYTES)?;
        check_header(payload)?;
        let request: Self = decode(payload)?;
        request.validate()?;
        Ok(request)
    }

    pub(super) fn validate(&self) -> Result<(), ProtocolError> {
        validate_header(self.protocol, self.request_id)?;
        match &self.command {
            Command::Switch { pack } => validate_pack(pack),
            Command::Volume { level } => validate_volume(*level),
            _ => Ok(()),
        }
    }
}

/// Bind this to the actual pipe-server process handle and creation time.
/// The per-start random nonce distinguishes instances; it is not a credential.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceIdentity {
    pub pid: u32,
    pub creation_time: u64,
    pub nonce: [u8; 16],
}

impl InstanceIdentity {
    pub(super) fn validate(&self) -> Result<(), ProtocolError> {
        if self.pid == 0 || self.creation_time == 0 || self.nonce == [0; 16] {
            return Err(ProtocolError::InvalidPayload("invalid instance identity"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    Starting,
    Ready,
    Paused,
    WaitingForAudio,
    Recovering,
    Failed,
    Stopping,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub state: EngineState,
    pub sound_pack: String,
    pub volume: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateState {
    /// False if shutdown was already requested before preparation was handled.
    pub restart: bool,
    pub paused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Busy,
    Timeout,
    ProtocolMismatch,
    InvalidRequest,
    InvalidPack,
    PersistenceFailure,
    Unavailable,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Success { snapshot: Snapshot },
    UpdatePrepared { state: UpdateState },
    Failure { code: ErrorCode, message: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub protocol: u32,
    pub request_id: u64,
    pub instance: InstanceIdentity,
    pub outcome: Outcome,
}

impl Response {
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        encode(self, MAX_RESPONSE_BYTES)
    }

    pub fn decode(frame: &[u8]) -> Result<Self, ProtocolError> {
        let payload = payload(frame, MAX_RESPONSE_BYTES)?;
        check_header(payload)?;
        let response: Self = decode(payload)?;
        response.validate()?;
        Ok(response)
    }

    /// The expected identity must come from a verified server, not a PID file.
    /// A Stop success is acceptance only; wait on that server's process handle.
    pub fn validate_for(
        &self,
        request: &Request,
        expected: &InstanceIdentity,
    ) -> Result<(), ProtocolError> {
        request.validate()?;
        self.validate()?;
        if self.request_id != request.request_id {
            return Err(ProtocolError::RequestMismatch);
        }
        if &self.instance != expected {
            return Err(ProtocolError::InstanceMismatch);
        }
        match (&request.command, &self.outcome) {
            (
                Command::PrepareUpdate {},
                Outcome::UpdatePrepared { .. } | Outcome::Failure { .. },
            ) => {}
            (Command::PrepareUpdate {}, _) | (_, Outcome::UpdatePrepared { .. }) => {
                return Err(ProtocolError::InvalidPayload(
                    "unexpected update preparation response",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        validate_header(self.protocol, self.request_id)?;
        self.instance.validate()?;
        match &self.outcome {
            Outcome::UpdatePrepared { .. } => Ok(()),
            Outcome::Success { snapshot } => {
                validate_pack(&snapshot.sound_pack)?;
                validate_volume(snapshot.volume)
            }
            Outcome::Failure { message, .. } => {
                if message.is_empty() || message.len() > MAX_ERROR_BYTES {
                    return Err(ProtocolError::InvalidPayload("invalid error message size"));
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProtocolError {
    TooLarge { limit: usize },
    IncompleteFrame,
    TrailingData,
    RejectedFrame,
    InvalidMessage,
    InvalidPayload(&'static str),
    ProtocolMismatch { received: u32, request_id: u64 },
    RequestMismatch,
    InstanceMismatch,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { limit } => write!(f, "control payload exceeds {limit} bytes"),
            Self::IncompleteFrame => f.write_str("incomplete control frame"),
            Self::TrailingData => f.write_str("only one control message is allowed per connection"),
            Self::RejectedFrame => f.write_str("control frame was already rejected"),
            Self::InvalidMessage => f.write_str("invalid control message"),
            Self::InvalidPayload(reason) => f.write_str(reason),
            Self::ProtocolMismatch { received, .. } => {
                write!(
                    f,
                    "unsupported control protocol {received}; expected {PROTOCOL_MAJOR}"
                )
            }
            Self::RequestMismatch => f.write_str("control response request ID does not match"),
            Self::InstanceMismatch => f.write_str("control response instance does not match"),
        }
    }
}

impl std::error::Error for ProtocolError {}

/// Incremental one-message buffer. Oversize/trailing-data failures are terminal.
/// This only frames bytes; callers still decode/validate and enforce a deadline.
pub struct FrameBuffer {
    bytes: Vec<u8>,
    limit: usize,
    complete: bool,
    rejected: bool,
}

impl FrameBuffer {
    pub fn request() -> Self {
        Self::new(MAX_REQUEST_BYTES)
    }

    pub fn response() -> Self {
        Self::new(MAX_RESPONSE_BYTES)
    }

    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit + 1),
            limit,
            complete: false,
            rejected: false,
        }
    }

    /// True means a complete frame; close after its one request/response exchange.
    pub fn feed(&mut self, chunk: &[u8]) -> Result<bool, ProtocolError> {
        if self.rejected {
            return Err(ProtocolError::RejectedFrame);
        }
        let result = self.append(chunk);
        if result.is_err() {
            self.rejected = true;
        }
        result
    }

    fn append(&mut self, chunk: &[u8]) -> Result<bool, ProtocolError> {
        if chunk.is_empty() {
            return Ok(self.complete);
        }
        if self.complete {
            return Err(ProtocolError::TrailingData);
        }
        let newline = chunk.iter().position(|byte| *byte == b'\n');
        if newline.is_some_and(|index| index + 1 != chunk.len()) {
            return Err(ProtocolError::TrailingData);
        }
        let payload_len = chunk.len() - usize::from(newline.is_some());
        if payload_len > self.limit - self.bytes.len() {
            return Err(ProtocolError::TooLarge { limit: self.limit });
        }
        self.bytes.extend_from_slice(chunk);
        self.complete = newline.is_some();
        Ok(self.complete)
    }

    pub fn finish(self) -> Result<Vec<u8>, ProtocolError> {
        if self.rejected {
            return Err(ProtocolError::RejectedFrame);
        }
        if !self.complete {
            return Err(ProtocolError::IncompleteFrame);
        }
        Ok(self.bytes)
    }
}

fn validate_header(protocol: u32, request_id: u64) -> Result<(), ProtocolError> {
    if request_id == 0 {
        return Err(ProtocolError::InvalidPayload("request ID must be nonzero"));
    }
    if protocol != PROTOCOL_MAJOR {
        return Err(ProtocolError::ProtocolMismatch {
            received: protocol,
            request_id,
        });
    }
    Ok(())
}

fn check_header(payload: &[u8]) -> Result<(), ProtocolError> {
    #[derive(Deserialize)]
    struct Header {
        protocol: u32,
        request_id: u64,
    }
    // Read the envelope before its body so a future command on another major
    // produces a protocol mismatch rather than an unknown-command diagnostic.
    let header: Header = decode(payload)?;
    validate_header(header.protocol, header.request_id)
}

fn validate_pack(pack: &str) -> Result<(), ProtocolError> {
    validate_pack_name(pack).map_err(|_| ProtocolError::InvalidPayload("invalid sound-pack name"))
}

fn validate_volume(volume: f32) -> Result<(), ProtocolError> {
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        return Err(ProtocolError::InvalidPayload(
            "volume must be between 0.0 and 1.0",
        ));
    }
    Ok(())
}

fn payload(frame: &[u8], limit: usize) -> Result<&[u8], ProtocolError> {
    let Some(payload) = frame.strip_suffix(b"\n") else {
        return Err(ProtocolError::IncompleteFrame);
    };
    if payload.len() > limit {
        return Err(ProtocolError::TooLarge { limit });
    }
    if payload.contains(&b'\n') {
        return Err(ProtocolError::TrailingData);
    }
    Ok(payload)
}

fn decode<'a, T: Deserialize<'a>>(payload: &'a [u8]) -> Result<T, ProtocolError> {
    serde_json::from_slice(payload).map_err(|_| ProtocolError::InvalidMessage)
}

fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, ProtocolError> {
    // Only validated, field-bounded Request/Response values reach this helper.
    let mut frame = serde_json::to_vec(value).map_err(|_| ProtocolError::InvalidMessage)?;
    if frame.len() > limit {
        return Err(ProtocolError::TooLarge { limit });
    }
    frame.push(b'\n');
    Ok(frame)
}
