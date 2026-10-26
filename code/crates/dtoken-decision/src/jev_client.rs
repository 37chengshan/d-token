use crate::{
    lint::lint_response,
    DecisionRequest, DecisionResponse, DecisionWorkflow, LintedDecision, WorkflowPolicy,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, fmt, net::IpAddr, sync::{Arc, Mutex}, time::Instant};

const DEFAULT_MAX_REQUEST_BYTES: usize = 64 * 1024;
const DEFAULT_MAX_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([redacted])")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientLimits {
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
}

impl Default for ClientLimits {
    fn default() -> Self {
        Self {
            max_request_bytes: DEFAULT_MAX_REQUEST_BYTES,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionHttpStatus {
    Unauthorized,
    RateLimited,
    ServerError,
    Other(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionFailureKind {
    NoCredential,
    InvalidEndpoint,
    InvalidCredential,
    RequestTooLarge,
    DeadlineExpired,
    Timeout,
    Connection,
    HttpStatus(DecisionHttpStatus),
    ResponseTooLarge,
    MalformedJson,
    InvalidSchema,
    WorkflowMismatch,
    DigestMismatch,
    CoverageMismatch,
    NumericOutOfRange,
    LowConfidence,
    CircuitOpen,
}

impl DecisionFailureKind {
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::NoCredential => "no_credential",
            Self::InvalidEndpoint => "invalid_endpoint",
            Self::InvalidCredential => "invalid_credential",
            Self::RequestTooLarge => "request_too_large",
            Self::DeadlineExpired => "deadline_expired",
            Self::Timeout => "timeout",
            Self::Connection => "connection_error",
            Self::HttpStatus(DecisionHttpStatus::Unauthorized) => "unauthorized",
            Self::HttpStatus(DecisionHttpStatus::RateLimited) => "rate_limited",
            Self::HttpStatus(DecisionHttpStatus::ServerError) => "server_error",
            Self::HttpStatus(DecisionHttpStatus::Other(_)) => "http_error",
            Self::ResponseTooLarge => "response_too_large",
            Self::MalformedJson => "malformed_json",
            Self::InvalidSchema => "unknown_schema",
            Self::WorkflowMismatch => "workflow_mismatch",
            Self::DigestMismatch => "digest_mismatch",
            Self::CoverageMismatch => "coverage_mismatch",
            Self::NumericOutOfRange => "numeric_out_of_range",
            Self::LowConfidence => "low_confidence",
            Self::CircuitOpen => "circuit_open",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionEventStatus {
    Applied,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionEvent {
    pub workflow: DecisionWorkflow,
    pub digest: String,
    pub status: DecisionEventStatus,
    pub latency_ms: u64,
    pub reason_code: String,
    pub http_status: Option<u16>,
}

impl DecisionEvent {
    fn applied(workflow: DecisionWorkflow, digest: &str, started: Instant) -> Self {
        Self {
            workflow,
            digest: digest.to_string(),
            status: DecisionEventStatus::Applied,
            latency_ms: elapsed_ms(started),
            reason_code: "applied".to_string(),
            http_status: None,
        }
    }

    fn failed(
        workflow: DecisionWorkflow,
        digest: &str,
        started: Instant,
        kind: &DecisionFailureKind,
        http_status: Option<u16>,
    ) -> Self {
        Self {
            workflow,
            digest: digest.to_string(),
            status: DecisionEventStatus::Failed,
            latency_ms: elapsed_ms(started),
            reason_code: kind.reason_code().to_string(),
            http_status,
        }
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionFailure {
    pub kind: DecisionFailureKind,
    pub event: DecisionEvent,
}

impl DecisionFailure {
    fn new(
        workflow: DecisionWorkflow,
        digest: &str,
        started: Instant,
        kind: DecisionFailureKind,
        http_status: Option<u16>,
    ) -> Self {
        let event = DecisionEvent::failed(workflow, digest, started, &kind, http_status);
        Self { kind, event }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientBuildError {
    #[error("invalid Jev endpoint")]
    InvalidEndpoint,
    #[error("request/response limits must be nonzero")]
    InvalidLimits,
    #[error("failed to build HTTP client")]
    HttpClient,
}

#[derive(Debug, Serialize)]
struct JevWireRequest {
    model: &'static str,
    state: JevState,
    questions: BTreeMap<String, JevQuestion>,
}

#[derive(Debug, Serialize)]
struct JevState {
    scope_digest: String,
    policy_digest: String,
    input_digest: String,
    candidate_digest: String,
    units: Vec<JevUnit>,
}

#[derive(Debug, Serialize)]
struct JevUnit {
    unit_id: String,
    kind: String,
    byte_len: u32,
    required: bool,
    protected: bool,
    age_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
struct JevQuestion {
    #[serde(rename = "type")]
    kind: &'static str,
    instructions: &'static str,
    criteria: BTreeMap<&'static str, &'static str>,
}

#[derive(Debug, Deserialize)]
struct JevWireResponse {
    answers: Map<String, Value>,
}

#[derive(Clone)]
pub struct JevClient {
    endpoint: String,
    secret: Option<SecretString>,
    limits: ClientLimits,
    http: reqwest::Client,
    last_event: Arc<Mutex<Option<DecisionEvent>>>,
    breakers: Arc<crate::CircuitBreakerSet>,
}

impl fmt::Debug for JevClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JevClient")
            .field("endpoint", &self.endpoint)
            .field("secret", &self.secret.as_ref().map(|_| "[redacted]"))
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl JevClient {
    pub fn new(
        endpoint: impl Into<String>,
        secret: Option<SecretString>,
        limits: ClientLimits,
    ) -> Result<Self, ClientBuildError> {
        Self::build(endpoint.into(), secret, limits, false)
    }

    /// Test-only construction permits an HTTP loopback endpoint. Production
    /// construction remains HTTPS-only, while unit tests can use a local TCP
    /// server without TLS fixtures.
    pub fn new_for_test(
        endpoint: impl Into<String>,
        secret: Option<SecretString>,
        limits: ClientLimits,
    ) -> Result<Self, ClientBuildError> {
        Self::build(endpoint.into(), secret, limits, true)
    }

    fn build(
        endpoint: String,
        secret: Option<SecretString>,
        limits: ClientLimits,
        allow_loopback_http: bool,
    ) -> Result<Self, ClientBuildError> {
        if limits.max_request_bytes == 0 || limits.max_response_bytes == 0 {
            return Err(ClientBuildError::InvalidLimits);
        }
        let parsed = reqwest::Url::parse(&endpoint).map_err(|_| ClientBuildError::InvalidEndpoint)?;
        if parsed.scheme() != "https"
            && !(allow_loopback_http
                && parsed.scheme() == "http"
                && parsed.host_str().is_some_and(is_loopback_host))
        {
            return Err(ClientBuildError::InvalidEndpoint);
        }
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| ClientBuildError::HttpClient)?;
        Ok(Self {
            endpoint,
            secret,
            limits,
            http,
            last_event: Arc::new(Mutex::new(None)),
            breakers: Arc::new(crate::CircuitBreakerSet::default()),
        })
    }

    pub fn last_event(&self) -> Option<DecisionEvent> {
        self.last_event
            .lock()
            .expect("Jev event mutex poisoned")
            .clone()
    }

    pub async fn decide(
        &self,
        workflow: DecisionWorkflow,
        input: &DecisionRequest,
        policy: &WorkflowPolicy,
        deadline: Instant,
    ) -> Result<LintedDecision, DecisionFailure> {
        let started = Instant::now();
        let digest = safe_digest(&input.input_digest);
        let fail = |kind: DecisionFailureKind, status: Option<u16>| {
            let failure = DecisionFailure::new(workflow, digest, started, kind, status);
            self.record_event(failure.event.clone());
            failure
        };

        if !workflow.is_executable() {
            return Err(fail(DecisionFailureKind::InvalidSchema, None));
        }
        if self.secret.as_ref().is_none_or(SecretString::is_empty) {
            return Err(fail(DecisionFailureKind::NoCredential, None));
        }
        let breaker = self
            .breakers
            .get(workflow)
            .expect("all executable workflows have a circuit breaker");
        if !breaker.allow(Instant::now()) {
            return Err(fail(DecisionFailureKind::CircuitOpen, None));
        }
        if let Err(error) = input.validate() {
            let kind = contract_failure_kind(&error);
            breaker.record_failure(Instant::now());
            return Err(fail(kind, None));
        }
        let wire = JevWireRequest {
            model: "jev-latest",
            state: JevState {
                scope_digest: input.metadata.scope_digest.clone(),
                policy_digest: input.metadata.policy_digest.clone(),
                input_digest: input.input_digest.clone(),
                candidate_digest: input.candidate_digest.clone(),
                units: input.units.iter().map(|unit| JevUnit {
                    unit_id: unit.unit_id.clone(), kind: unit.kind.clone(), byte_len: unit.byte_len,
                    required: unit.required, protected: unit.protected, age_ms: unit.age_ms,
                }).collect(),
            },
            questions: input.units.iter().enumerate().map(|(index, _)| {
                let mut criteria = BTreeMap::new();
                criteria.insert("keep", "retain this unit in the context");
                criteria.insert("omit", "omit this unit when the local policy permits");
                (format!("u{index}"), JevQuestion {
                    kind: "choice", instructions: "Should this candidate unit be retained for the current workflow?",
                    criteria,
                })
            }).collect(),
        };
        let body = match serde_json::to_vec(&wire) {
            Ok(body) if body.len() <= self.limits.max_request_bytes => body,
            Ok(_) => { breaker.record_failure(Instant::now()); return Err(fail(DecisionFailureKind::RequestTooLarge, None)); },
            Err(_) => { breaker.record_failure(Instant::now()); return Err(fail(DecisionFailureKind::InvalidSchema, None)); },
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            breaker.record_failure(Instant::now());
            return Err(fail(DecisionFailureKind::DeadlineExpired, None));
        }
        let secret = self.secret.as_ref().expect("credential checked above");
        let authorization = match HeaderValue::from_str(&format!("Bearer {}", secret.as_str())) {
            Ok(value) => value,
            Err(_) => { breaker.record_failure(Instant::now()); return Err(fail(DecisionFailureKind::InvalidCredential, None)); },
        };
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(AUTHORIZATION, authorization);
        let request = self
            .http
            .post(&self.endpoint)
            .headers(headers)
            .body(body);
        let response = match tokio::time::timeout(remaining, request.send()).await {
            Ok(Ok(response)) => response,
            Ok(Err(error)) if error.is_timeout() => {
                breaker.record_failure(Instant::now()); return Err(fail(DecisionFailureKind::Timeout, None))
            }
            Ok(Err(_)) => { breaker.record_failure(Instant::now()); return Err(fail(DecisionFailureKind::Connection, None)) },
            Err(_) => { breaker.record_failure(Instant::now()); return Err(fail(DecisionFailureKind::Timeout, None)) },
        };
        let status = response.status();
        if !status.is_success() {
            let mapped = if status.as_u16() == 401 || status.as_u16() == 403 {
                DecisionHttpStatus::Unauthorized
            } else if status.as_u16() == 429 {
                DecisionHttpStatus::RateLimited
            } else if status.is_server_error() {
                DecisionHttpStatus::ServerError
            } else {
                DecisionHttpStatus::Other(status.as_u16())
            };
            breaker.record_failure(Instant::now());
            return Err(fail(
                DecisionFailureKind::HttpStatus(mapped),
                Some(status.as_u16()),
            ));
        }
        let body = match read_bounded_body(response, self.limits.max_response_bytes, deadline).await {
            Ok(body) => body,
            Err(kind) => { breaker.record_failure(Instant::now()); return Err(fail(kind, Some(status.as_u16()))) },
        };
        let response: JevWireResponse = match serde_json::from_slice(&body) {
            Ok(response) => response,
            Err(_) => { breaker.record_failure(Instant::now()); return Err(fail(DecisionFailureKind::MalformedJson, Some(status.as_u16()))) },
        };
        let response = match map_jev_response(response, workflow, input) {
            Ok(response) => response,
            Err(kind) => { breaker.record_failure(Instant::now()); return Err(fail(kind, Some(status.as_u16()))) },
        };
        let decision = match lint_response(&response, workflow, input, policy) {
            Ok(decision) => decision,
            Err(error) => { breaker.record_failure(Instant::now()); return Err(fail(lint_failure_kind(&error), Some(status.as_u16()))) },
        };
        breaker.record_success();
        self.record_event(DecisionEvent::applied(workflow, digest, started));
        Ok(decision)
    }

    fn record_event(&self, event: DecisionEvent) {
        *self.last_event.lock().expect("Jev event mutex poisoned") = Some(event.clone());
        tracing::debug!(
            workflow = event.workflow.as_str(),
            digest = %event.digest,
            status = ?event.status,
            latency_ms = event.latency_ms,
            reason = %event.reason_code,
            http_status = ?event.http_status,
            "jev decision event"
        );
    }
}

#[async_trait]
pub trait DecisionClient: Send + Sync {
    async fn decide(
        &self,
        workflow: DecisionWorkflow,
        input: &DecisionRequest,
        policy: &WorkflowPolicy,
        deadline: Instant,
    ) -> Result<LintedDecision, DecisionFailure>;
}

#[async_trait]
impl DecisionClient for JevClient {
    async fn decide(
        &self,
        workflow: DecisionWorkflow,
        input: &DecisionRequest,
        policy: &WorkflowPolicy,
        deadline: Instant,
    ) -> Result<LintedDecision, DecisionFailure> {
        JevClient::decide(self, workflow, input, policy, deadline).await
    }
}

async fn read_bounded_body(
    response: reqwest::Response,
    max_bytes: usize,
    deadline: Instant,
) -> Result<Vec<u8>, DecisionFailureKind> {
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = tokio::time::timeout(
        deadline.saturating_duration_since(Instant::now()),
        stream.next(),
    )
    .await
    .map_err(|_| DecisionFailureKind::Timeout)?
    .map(|result| result.map_err(|_| DecisionFailureKind::Connection))
    .transpose()?
    {
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Err(DecisionFailureKind::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn safe_digest(value: &str) -> &str {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        value
    } else {
        "invalid_digest"
    }
}

fn map_jev_response(
    response: JevWireResponse,
    workflow: DecisionWorkflow,
    input: &DecisionRequest,
) -> Result<DecisionResponse, DecisionFailureKind> {
    if response.answers.len() != input.units.len() {
        return Err(DecisionFailureKind::CoverageMismatch);
    }
    let mut units = Vec::with_capacity(input.units.len());
    let mut confidence_sum = 0.0_f32;
    for (index, unit) in input.units.iter().enumerate() {
        let key = format!("u{index}");
        let answer = response
            .answers
            .get(&key)
            .ok_or(DecisionFailureKind::CoverageMismatch)?;
        let (selected, score, confidence) = map_jev_answer(answer)?;
        confidence_sum += confidence;
        units.push(crate::DecisionUnitDecision {
            unit_id: unit.unit_id.clone(),
            selected,
            score,
        });
    }
    Ok(DecisionResponse {
        schema_version: crate::DECISION_SCHEMA_VERSION,
        workflow,
        request_id: input.request_id.clone(),
        input_digest: input.input_digest.clone(),
        candidate_digest: input.candidate_digest.clone(),
        confidence: confidence_sum / input.units.len() as f32,
        units,
    })
}

fn map_jev_answer(answer: &Value) -> Result<(bool, f32, f32), DecisionFailureKind> {
    let object = answer
        .as_object()
        .ok_or(DecisionFailureKind::InvalidSchema)?;
    if let Some(value) = object.get("noul").and_then(Value::as_f64) {
        let value = finite_probability(value)?;
        return Ok((value >= 0.5, value, value.max(1.0 - value)));
    }
    if let Some(choice) = object.get("choice").and_then(Value::as_str) {
        let confidence = object
            .get("confidence")
            .and_then(Value::as_f64)
            .ok_or(DecisionFailureKind::InvalidSchema)?;
        let confidence = finite_probability(confidence)?;
        let selected = matches!(choice, "keep" | "retain" | "select" | "yes" | "true");
        let score = object
            .get("probabilities")
            .and_then(Value::as_object)
            .and_then(|probabilities| probabilities.get(choice))
            .and_then(Value::as_f64)
            .map(finite_probability)
            .transpose()?
            .unwrap_or(if selected { confidence } else { 1.0 - confidence });
        return Ok((selected, score, confidence));
    }
    if let Some(score) = object.get("score").and_then(Value::as_f64) {
        let score = finite_probability(score)?;
        let confidence = object
            .get("confidence")
            .and_then(Value::as_f64)
            .ok_or(DecisionFailureKind::InvalidSchema)
            .and_then(finite_probability)?;
        return Ok((score >= 0.5, score, confidence));
    }
    Err(DecisionFailureKind::InvalidSchema)
}

fn finite_probability(value: f64) -> Result<f32, DecisionFailureKind> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(DecisionFailureKind::NumericOutOfRange);
    }
    Ok(value as f32)
}

fn contract_failure_kind(error: &crate::contract::ContractError) -> DecisionFailureKind {
    use crate::contract::ContractError;
    match error {
        ContractError::NumericOutOfRange(_) => DecisionFailureKind::NumericOutOfRange,
        _ => DecisionFailureKind::InvalidSchema,
    }
}

fn lint_failure_kind(error: &crate::lint::LintError) -> DecisionFailureKind {
    use crate::lint::LintError;
    match error {
        LintError::UnsupportedSchemaVersion(_) => DecisionFailureKind::InvalidSchema,
        LintError::WorkflowMismatch => DecisionFailureKind::WorkflowMismatch,
        LintError::DigestMismatch => DecisionFailureKind::DigestMismatch,
        LintError::CoverageMismatch | LintError::DuplicateUnitId(_) => {
            DecisionFailureKind::CoverageMismatch
        }
        LintError::ConfidenceOutOfRange | LintError::ScoreOutOfRange(_) => {
            DecisionFailureKind::NumericOutOfRange
        }
        LintError::LowConfidence => DecisionFailureKind::LowConfidence,
        LintError::RequestIdMismatch | LintError::CandidateDigestMismatch => {
            DecisionFailureKind::DigestMismatch
        }
        LintError::InvalidRequest(_) => DecisionFailureKind::InvalidSchema,
    }
}
