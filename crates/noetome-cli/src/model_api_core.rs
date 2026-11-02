fn require(value: bool, code: &'static str) -> Result<(), ModelError> {
    if value { Ok(()) } else { Err(code.into()) }
}
fn parse<T: for<'de> Deserialize<'de>>(bytes: &[u8], code: &'static str) -> Result<T, ModelError> {
    serde_json::from_slice(bytes).map_err(|_| code.into())
}
fn json<T: Serialize>(value: &T) -> Result<Vec<u8>, ModelError> {
    serde_json::to_vec_pretty(value).map_err(|_| "MODEL_API_SERIALIZATION_FAILED".into())
}
fn fingerprint(bytes: &[u8]) -> ArtifactFingerprint {
    ArtifactFingerprint {
        sha256: sidecar::digest(bytes),
        byte_length: bytes.len() as u64,
    }
}
fn check_fingerprint(bytes: &[u8], expected: &ArtifactFingerprint) -> Result<(), ModelError> {
    require(fingerprint(bytes) == *expected, "MODEL_API_SAVED_INVALID")
}
fn unknown_usage() -> Usage {
    Usage {
        provenance: UsageProvenance::ProviderReported,
        input_tokens: None,
        output_tokens: None,
        cost: Cost {
            status: CostStatus::Unknown,
            amount_microunits: None,
            currency: None,
            basis: CostBasis::NotAvailable,
        },
    }
}
fn budget_exceeded(usage: &Usage, reported_total: Option<u64>, budget: u64) -> bool {
    // A reported component is already a lower bound on total usage. Do not
    // invent a provider total when it is unknown, or let that null hide a known
    // component (or known component sum) above the authorized total budget.
    reported_total.is_some_and(|total| total > budget)
        || usage.input_tokens.is_some_and(|input| input > budget)
        || usage.output_tokens.is_some_and(|output| output > budget)
        || match (usage.input_tokens, usage.output_tokens) {
            (Some(input), Some(output)) => input.checked_add(output).is_none_or(|sum| sum > budget),
            _ => false,
        }
}
fn provider() -> ProviderIdentity {
    ProviderIdentity {
        producer: Producer::Model,
        provider_id: PROVIDER_ID.into(),
        adapter_version: "responses-stdlib-v1".into(),
        requested_model: MODEL.into(),
    }
}
fn reservation(config: &ApiConfiguration) -> Result<TotalTokenReservation, ModelError> {
    require(
        config.schema_version == 1
            && config.protocol == "noetome-api-config-v1"
            && config.provider_id == PROVIDER_ID
            && config.endpoint == ENDPOINT
            && config.model == MODEL
            && config.credential_ref == CREDENTIAL_REF,
        "MODEL_API_CONFIG_INVALID",
    )?;
    let ceiling = Policy::default();
    let p = &config.policy;
    require(
        p.max_input_bytes > 0
            && p.max_input_bytes <= ceiling.max_input_bytes
            && p.max_response_bytes > 0
            && p.max_response_bytes <= ceiling.max_response_bytes
            && p.max_output_tokens > 0
            && p.max_output_tokens <= ceiling.max_output_tokens
            && p.timeout_ms > 0
            && p.timeout_ms <= ceiling.timeout_ms
            && p.max_calls == 1
            && p.retries == 0
            && !p.search_enabled
            && !p.tools_enabled,
        "MODEL_API_CONFIG_INVALID",
    )?;
    let total_tokens = 65536 + p.max_output_tokens;
    require(
        config.total_token_budget >= total_tokens,
        "MODEL_API_BUDGET_INSUFFICIENT",
    )?;
    Ok(TotalTokenReservation {
        calls: 1,
        input_token_upper_bound: 65536,
        output_tokens: p.max_output_tokens,
        total_tokens,
        basis: "http_body_utf8_byte_cap_plus_max_output_tokens_client_policy".into(),
    })
}
fn authorize(
    config: &ApiConfiguration,
    config_sha: &str,
    auth: &SourceAuthorization,
    summary: &FullTextSummary,
) -> Result<(), ModelError> {
    require(
        auth.schema_version == 1
            && auth.protocol == "noetome-api-authorized-source-v1"
            && source_model::valid_uuid(&auth.authorization_id)
            && auth.configuration_sha256 == config_sha
            && auth.endpoint == config.endpoint
            && auth.model == config.model
            && auth.credential_ref == config.credential_ref
            && auth.budget_total_tokens == config.total_token_budget
            && auth.max_calls == 1
            && auth.allow_externalization
            && auth.source == *summary
            && source_model::valid_time(&auth.authorized_at),
        "MODEL_API_AUTHORIZATION_INVALID",
    )
}
fn request(
    prepared: &PreparedText,
    config: &ApiConfiguration,
    id: String,
) -> Result<ExtractionRequest, ModelError> {
    let mut source = prepared.evidence_source();
    source.units = prepared.units().to_vec();
    let request = ExtractionRequest {
        schema_version: 1,
        protocol: model_candidate::PROTOCOL.into(),
        request_id: id,
        provider: provider(),
        source,
        policy: config.policy.clone(),
    };
    model_candidate::validate_request(&request)
        .map_err(|_| ModelError::from("MODEL_API_SOURCE_INVALID"))?;
    require(
        prepared.text().len() as u64 <= request.policy.max_input_bytes,
        "MODEL_API_SOURCE_INVALID",
    )?;
    Ok(request)
}
fn fixed_adapter() -> Result<PathBuf, ModelError> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sidecars/responses_api/adapter.py")
        .canonicalize()
        .map_err(|_| "MODEL_API_RUNTIME_INVALID".into())
}
struct Admission {
    project: PathBuf,
    prepared: PreparedText,
    configuration: ApiConfiguration,
    config: guard::Input,
    auth: guard::Input,
    python: guard::Input,
    adapter: guard::Input,
    source_files: Vec<guard::Input>,
    schema: guard::Input,
    instructions: guard::Input,
    request: ExtractionRequest,
    wire: Vec<u8>,
    reservation: TotalTokenReservation,
}
impl Admission {
    fn open(
        project: &Path,
        version: &str,
        config_path: &Path,
        auth_path: &Path,
        python_path: &Path,
    ) -> Result<Self, ModelError> {
        let config = guard::Input::open(config_path, 65536)
            .map_err(|_| ModelError::from("MODEL_API_CONFIG_INVALID"))?;
        let configuration: ApiConfiguration = parse(&config.bytes, "MODEL_API_CONFIG_INVALID")?;
        let reservation = reservation(&configuration)?;
        let project =
            guard::path(project).map_err(|_| ModelError::from("MODEL_API_SOURCE_INVALID"))?;
        let prepared = model_preflight::prepare_whole_text(&project, version)
            .map_err(|_| ModelError::from("MODEL_API_SOURCE_INVALID"))?;
        let auth = guard::Input::open(auth_path, 65536)
            .map_err(|_| ModelError::from("MODEL_API_AUTHORIZATION_INVALID"))?;
        let authorization: SourceAuthorization =
            parse(&auth.bytes, "MODEL_API_AUTHORIZATION_INVALID")?;
        authorize(
            &configuration,
            &config.fingerprint.sha256,
            &authorization,
            prepared.summary(),
        )?;
        let request = request(&prepared, &configuration, uuid::Uuid::new_v4().to_string())?;
        let wire = json(&request)?;
        require(wire.len() <= 65536, "MODEL_API_REQUEST_TOO_LARGE")?;
        require(python_path.is_absolute(), "MODEL_API_RUNTIME_INVALID")?;
        let python = guard::Input::open(python_path, 128 * 1024 * 1024)
            .map_err(|_| ModelError::from("MODEL_API_RUNTIME_INVALID"))?;
        let adapter = guard::Input::open(&fixed_adapter()?, 1024 * 1024)
            .map_err(|_| ModelError::from("MODEL_API_RUNTIME_INVALID"))?;
        require(
            adapter.bytes == include_bytes!("../../../sidecars/responses_api/adapter.py"),
            "MODEL_API_RUNTIME_INVALID",
        )?;
        let source_files = source_guards(&project, version)?;
        let assets = adapter
            .path
            .parent()
            .ok_or(ModelError::from("MODEL_API_RUNTIME_INVALID"))?;
        let schema = guard::Input::open(&assets.join("schema.json"), 65536)
            .map_err(|_| ModelError::from("MODEL_API_RUNTIME_INVALID"))?;
        let instructions = guard::Input::open(&assets.join("instructions.txt"), 65536)
            .map_err(|_| ModelError::from("MODEL_API_RUNTIME_INVALID"))?;
        require(
            schema.bytes == include_bytes!("../../../sidecars/responses_api/schema.json")
                && instructions.bytes
                    == include_bytes!("../../../sidecars/responses_api/instructions.txt"),
            "MODEL_API_RUNTIME_INVALID",
        )?;
        let admitted = Self {
            project,
            prepared,
            configuration,
            config,
            auth,
            python,
            adapter,
            source_files,
            schema,
            instructions,
            request,
            wire,
            reservation,
        };
        admitted.verify()?;
        Ok(admitted)
    }
    fn verify(&self) -> Result<(), ModelError> {
        self.config.verify()?;
        self.auth.verify()?;
        self.python.verify()?;
        self.adapter.verify()?;
        self.schema.verify()?;
        self.instructions.verify()?;
        for file in &self.source_files {
            file.verify()?;
        }
        self.prepared
            .verify()
            .map_err(|_| "MODEL_API_SOURCE_CHANGED".into())
    }
    fn runtime(&self) -> RuntimeFingerprint {
        RuntimeFingerprint {
            python_path: self.python.path.to_string_lossy().into_owned(),
            python: self.python.fingerprint.clone(),
            adapter_path: self.adapter.path.to_string_lossy().into_owned(),
            adapter: self.adapter.fingerprint.clone(),
            schema: self.schema.fingerprint.clone(),
            instructions: self.instructions.fingerprint.clone(),
        }
    }
    fn preflight_report(&self) -> PreflightReport {
        PreflightReport {
            schema_version: 1,
            protocol: "noetome-api-preflight-v1".into(),
            provider_id: PROVIDER_ID.into(),
            endpoint: ENDPOINT.into(),
            model: MODEL.into(),
            credential_ref: CREDENTIAL_REF.into(),
            source: self.prepared.summary().clone(),
            configuration_path: self.config.path.to_string_lossy().into_owned(),
            configuration: self.config.fingerprint.clone(),
            authorization_path: self.auth.path.to_string_lossy().into_owned(),
            authorization: self.auth.fingerprint.clone(),
            runtime: self.runtime(),
            reservation: self.reservation.clone(),
            budget_total_tokens: self.configuration.total_token_budget,
            no_api_calls: true,
            key_loaded: false,
        }
    }
}
fn source_guards(project_root: &Path, version: &str) -> Result<Vec<guard::Input>, ModelError> {
    let info =
        project::inspect(project_root).map_err(|_| ModelError::from("MODEL_API_SOURCE_INVALID"))?;
    let (store, _, _) = source::database_state(
        &project_root.join(project::DATABASE),
        project_root,
        &info,
        None,
        None,
    )
    .map_err(|_| ModelError::from("MODEL_API_SOURCE_INVALID"))?;
    let version = store
        .versions
        .get(version)
        .ok_or(ModelError::from("MODEL_API_SOURCE_INVALID"))?;
    let paths = [
        (project_root.join(project::CONFIG), 65536),
        (project_root.join(project::DATABASE), 128 * 1024 * 1024),
        (project_root.join(&version.raw_snapshot), 64 * 1024 * 1024),
        (
            project_root.join(
                version
                    .normalized_snapshot
                    .as_ref()
                    .ok_or(ModelError::from("MODEL_API_SOURCE_INVALID"))?,
            ),
            8192,
        ),
    ];
    paths
        .into_iter()
        .map(|(path, cap)| {
            guard::Input::open(&path, cap).map_err(|_| "MODEL_API_SOURCE_INVALID".into())
        })
        .collect()
}
pub fn preflight(
    project: &Path,
    version: &str,
    config: &Path,
    authorization: &Path,
    output: &Path,
    python: &Path,
) -> Result<PreflightReport, ModelError> {
    let admission = Admission::open(project, version, config, authorization, python)?;
    api_store::preflight(project, output)?;
    admission.verify()?;
    Ok(admission.preflight_report())
}
fn credential_error(error: credential_handoff::CredentialError) -> ModelError {
    match error.code {
        "MODEL_CREDENTIAL_MISSING" => "MODEL_API_CREDENTIAL_MISSING".into(),
        "MODEL_CREDENTIAL_ECHO" => "MODEL_API_CREDENTIAL_ECHO".into(),
        _ => "MODEL_API_CREDENTIAL_INVALID".into(),
    }
}
fn no_echo(credential: &TransientCredential, bytes: &[u8]) -> Result<(), ModelError> {
    credential.validate_no_echo(bytes).map_err(credential_error)
}
pub fn extract(
    project: &Path,
    version: &str,
    config: &Path,
    authorization: &Path,
    output: &Path,
    python: &Path,
) -> Result<ApiReport, ModelError> {
    let admission = Admission::open(project, version, config, authorization, python)?;
    api_store::preflight(project, output)?;
    let credential = TransientCredential::from_env_ref(CREDENTIAL_REF).map_err(credential_error)?;
    let program = admission.python.path.clone();
    let script = admission.adapter.path.clone();
    run_admitted(
        admission,
        output,
        &credential,
        "live",
        |request, wire, runtime, key| {
            credential_handoff::run(
                &program,
                &[OsString::from("-B"), script.into_os_string()],
                request,
                wire,
                runtime,
                key,
                &AtomicBool::new(false),
            )
        },
    )
}

named!(AttemptMarker {
    schema_version: u32,
    protocol: String,
    request_id: String,
    request_sha256: String,
    max_calls: u64,
    http_state: String,
    execution_kind: String
});
struct RuntimeRoot {
    root: PathBuf,
    dirs: Vec<guard::Directory>,
}
impl RuntimeRoot {
    fn create(output: &Path) -> Result<Self, ModelError> {
        let root = output
            .parent()
            .ok_or(ModelError::from("MODEL_API_OUTPUT_INVALID"))?
            .join(format!(".noetome-api-runtime-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).map_err(|_| ModelError::from("MODEL_API_RUNTIME_INVALID"))?;
        let dirs = guard::chain(&root)?;
        Ok(Self { root, dirs })
    }
    fn verify(&self) -> Result<(), ModelError> {
        guard::verify_chain(&self.dirs)?;
        for entry in
            fs::read_dir(&self.root).map_err(|_| ModelError::from("MODEL_API_RUNTIME_INVALID"))?
        {
            let name = entry
                .map_err(|_| ModelError::from("MODEL_API_RUNTIME_INVALID"))?
                .file_name();
            let name = name
                .to_str()
                .ok_or(ModelError::from("MODEL_API_RUNTIME_INVALID"))?;
            require(
                matches!(
                    name,
                    "http-request.json" | "http-observation.json" | "http-response.bin"
                ),
                "MODEL_API_RUNTIME_INVALID",
            )?;
        }
        guard::verify_chain(&self.dirs)
    }
}
fn offline_body(
    admission: &Admission,
    store: &api_store::ApiStore,
    runtime: &RuntimeRoot,
) -> Result<Vec<u8>, ModelError> {
    let output = sidecar_process::execute_command(
        &admission.python.path,
        &[
            OsString::from("-B"),
            admission.adapter.path.as_os_str().to_owned(),
            OsString::from("--offline-body"),
            store.root().join("request.json").into_os_string(),
        ],
        &[],
        &runtime.root,
        &sidecar::SidecarLimits {
            max_request_bytes: 65536,
            max_stdout_bytes: 65536,
            max_stderr_bytes: 65536,
            timeout: Duration::from_millis(admission.configuration.policy.timeout_ms),
            ..Default::default()
        },
        &AtomicBool::new(false),
    )
    .map_err(|_| ModelError::from("MODEL_API_HTTP_BODY_INVALID"))?;
    require(output.stderr.is_empty(), "MODEL_API_HTTP_BODY_INVALID")?;
    decode::validate_body(&output.stdout, &admission.request)?;
    Ok(output.stdout)
}

struct Capture {
    observation: HttpObservation,
    inputs: Vec<guard::Input>,
    raw_index: Option<usize>,
    canonical: Option<Vec<u8>>,
    derived: Option<Outcome>,
}
fn capture(
    admission: &Admission,
    runtime: &RuntimeRoot,
    body: &[u8],
    attempt: &SensitiveAttempt,
    key: &TransientCredential,
) -> Result<Capture, ModelError> {
    let actual_body = guard::Input::open(&runtime.root.join("http-request.json"), 65536)
        .map_err(|_| ModelError::from("MODEL_API_OBSERVATION_INVALID"))?;
    no_echo(key, &actual_body.bytes)?;
    require(actual_body.bytes == body, "MODEL_API_HTTP_BODY_INVALID")?;
    let observation = guard::Input::open(&runtime.root.join("http-observation.json"), 65536)
        .map_err(|_| ModelError::from("MODEL_API_OBSERVATION_INVALID"))?;
    no_echo(key, &observation.bytes)?;
    let obs: HttpObservation = parse(&observation.bytes, "MODEL_API_OBSERVATION_INVALID")?;
    decode::validate_observation(
        &obs,
        &admission.request,
        &sidecar::digest(&admission.wire),
        &sidecar::digest(body),
    )?;
    let mut inputs = vec![actual_body, observation];
    let mut raw_index = None;
    let mut derived = None;
    let mut canonical = None;
    if obs.raw_response_available {
        let raw = guard::Input::open(
            &runtime.root.join("http-response.bin"),
            admission.configuration.policy.max_response_bytes,
        )
        .map_err(|_| ModelError::from("MODEL_API_RESPONSE_INVALID"))?;
        no_echo(key, &raw.bytes)?;
        let expected = decode::derive(
            &admission.request,
            &admission.wire,
            body,
            &raw.bytes,
            obs.http_status
                .ok_or(ModelError::from("MODEL_API_OBSERVATION_INVALID"))?,
        )?;
        require(expected.1 == obs, "MODEL_API_OBSERVATION_INVALID")?;
        derived = expected.0;
        raw_index = Some(inputs.len());
        inputs.push(raw);
    } else {
        require(
            !runtime.root.join("http-response.bin").exists(),
            "MODEL_API_OBSERVATION_INVALID",
        )?;
    }
    if obs.canonical_response_available {
        let bytes = attempt
            .response
            .as_ref()
            .ok_or(ModelError::from("MODEL_API_RESPONSE_INVALID"))?;
        no_echo(key, bytes)?;
        let outcome = model_candidate::decode_auditable_outcome(
            bytes,
            &admission.request,
            &sidecar::digest(&admission.wire),
        )
        .map_err(|_| ModelError::from("MODEL_API_RESPONSE_INVALID"))?;
        require(
            derived.as_ref() == Some(&outcome),
            "MODEL_API_RESPONSE_INVALID",
        )?;
        canonical = Some(bytes.clone());
    } else {
        require(attempt.response.is_none(), "MODEL_API_OBSERVATION_INVALID")?;
    }
    for input in &inputs {
        input.verify()?;
    }
    runtime.verify()?;
    Ok(Capture {
        observation: obs,
        inputs,
        raw_index,
        canonical,
        derived,
    })
}
fn run_admitted(
    admission: Admission,
    output: &Path,
    credential: &TransientCredential,
    execution_kind: &str,
    invoke: impl FnOnce(&ExtractionRequest, &[u8], &Path, &TransientCredential) -> SensitiveAttempt,
) -> Result<ApiReport, ModelError> {
    no_echo(credential, &admission.config.bytes)?;
    no_echo(credential, &admission.auth.bytes)?;
    no_echo(credential, &admission.wire)?;
    let cancel = AtomicBool::new(false);
    admission.verify()?;
    let request_sha = sidecar::digest(&admission.wire);
    let marker = AttemptMarker {
        schema_version: 1,
        protocol: "noetome-api-attempt-v1".into(),
        request_id: admission.request.request_id.clone(),
        request_sha256: request_sha.clone(),
        max_calls: 1,
        http_state: "remote_unknown".into(),
        execution_kind: execution_kind.into(),
    };
    let reservation_bytes = json(&admission.reservation)?;
    no_echo(credential, &reservation_bytes)?;
    let marker_bytes = json(&marker)?;
    no_echo(credential, &marker_bytes)?;
    let mut store = api_store::ApiStore::create(
        &admission.project,
        output,
        &admission.request.request_id,
        &request_sha,
        &cancel,
    )?;
    store.put("config.json", &admission.config.bytes, &cancel)?;
    store.put("authorization.json", &admission.auth.bytes, &cancel)?;
    store.put("request.json", &admission.wire, &cancel)?;
    store.put("reservation.json", &reservation_bytes, &cancel)?;
    store.put("attempt.json", &marker_bytes, &cancel)?;
    let runtime = RuntimeRoot::create(store.root())?;
    let body = offline_body(&admission, &store, &runtime)?;
    no_echo(credential, &body)?;
    store.put("http-request.json", &body, &cancel)?;
    admission.verify()?;
    store.verify(&cancel)?;
    runtime.verify()?;
    let attempt = invoke(
        &admission.request,
        &admission.wire,
        &runtime.root,
        credential,
    );
    admission.verify()?;
    runtime.verify()?;
    store.verify(&cancel)?;
    let mut report = ApiReport {
        schema_version: 2,
        protocol: "noetome-api-single-v1".into(),
        execution_kind: execution_kind.into(),
        request_id: admission.request.request_id.clone(),
        request_sha256: request_sha.clone(),
        configuration_path: admission.config.path.to_string_lossy().into_owned(),
        configuration: admission.config.fingerprint.clone(),
        authorization_path: admission.auth.path.to_string_lossy().into_owned(),
        authorization: admission.auth.fingerprint.clone(),
        source: admission.prepared.summary().clone(),
        runtime: admission.runtime(),
        reservation: admission.reservation.clone(),
        budget_total_tokens: admission.configuration.total_token_budget,
        attempt: ApiAttempt {
            status: attempt.status,
            pid: attempt.pid,
            elapsed_ms: attempt.elapsed_ms,
            failure: attempt.failure.map(|_| "MODEL_API_TRANSPORT_FAILED".into()),
            stdout_suppressed: attempt.stdout_suppressed,
            stderr_suppressed: true,
        },
        observation: None,
        http_request: Some(fingerprint(&body)),
        raw_response: None,
        canonical_response: None,
        candidate: None,
        candidate_id: None,
        http_state: "remote_unknown".into(),
        confirmed_http_posts: if execution_kind == "offline_test" {
            Some(0)
        } else {
            None
        },
        http_status: None,
        response_id: None,
        actual_model: None,
        usage_total_tokens: None,
        usage: unknown_usage(),
        cost_unknown: true,
        candidate_success: false,
        verification: "not_applicable".into(),
        failure: Some("MODEL_API_REMOTE_UNKNOWN".into()),
    };
    let captured = capture(&admission, &runtime, &body, &attempt, credential);
    match &captured {
        Ok(captured) => {
            let obs = &captured.observation;
            store.put("http-observation.json", &captured.inputs[1].bytes, &cancel)?;
            report.http_state = obs.http_state.clone();
            report.confirmed_http_posts = if execution_kind == "offline_test" {
                Some(0)
            } else {
                match obs.http_state.as_str() {
                    "not_sent" => Some(0),
                    "sent_once" => Some(1),
                    _ => None,
                }
            };
            report.http_status = obs.http_status;
            report.response_id = obs.response_id.clone();
            report.actual_model = obs.actual_model.clone();
            report.usage.input_tokens = obs.usage_input_tokens;
            report.usage.output_tokens = obs.usage_output_tokens;
            report.usage_total_tokens = obs.usage_total_tokens;
            report.failure = obs
                .failure
                .as_deref()
                .map(decode::failure_code)
                .map(str::to_owned);
            if let Some(index) = captured.raw_index {
                let raw = &captured.inputs[index];
                store.put("http-response.bin", &raw.bytes, &cancel)?;
                report.raw_response = Some(raw.fingerprint.clone());
            }
            if let Some(bytes) = &captured.canonical {
                store.put("response.json", bytes, &cancel)?;
                report.canonical_response = Some(fingerprint(bytes));
                let outcome = captured
                    .derived
                    .as_ref()
                    .ok_or(ModelError::from("MODEL_API_RESPONSE_INVALID"))?;
                report.usage = outcome.usage.clone();
                let mut evidence_request = admission.request.clone();
                evidence_request.source = admission.prepared.evidence_source();
                if budget_exceeded(
                    &report.usage,
                    report.usage_total_tokens,
                    report.budget_total_tokens,
                ) {
                    report.failure = Some("MODEL_API_USAGE_BUDGET_EXCEEDED".into());
                } else {
                    match model_candidate::make_candidate_ir(
                        outcome,
                        &evidence_request,
                        &request_sha,
                        &sidecar::digest(bytes),
                    ) {
                        Ok(ir) => {
                            let bytes = json(&ir)?;
                            no_echo(credential, &bytes)?;
                            store.put("candidate.json", &bytes, &cancel)?;
                            report.candidate = Some(fingerprint(&bytes));
                            report.candidate_id = Some(ir.candidate_id);
                            report.candidate_success = true;
                            report.verification = "unverified".into();
                            report.failure = None;
                        }
                        Err(_) => report.failure = Some("MODEL_API_EVIDENCE_INVALID".into()),
                    }
                }
            }
            report.observation = Some(obs.clone());
        }
        Err(error) => report.failure = Some(error.code.into()),
    }
    admission.verify()?;
    runtime.verify()?;
    store.verify(&cancel)?;
    if let Ok(captured) = &captured {
        for input in &captured.inputs {
            input.verify()?;
        }
    }
    let bytes = json(&report)?;
    no_echo(credential, &bytes)?;
    store.commit_report(&bytes, &cancel)?;
    require(
        store.read_owned("report.json")? == bytes,
        "MODEL_API_OUTPUT_CHANGED",
    )?;
    admission.verify()?;
    if let Ok(captured) = &captured {
        for input in &captured.inputs {
            input.verify()?;
        }
    }
    Ok(report)
}

pub fn inspect(project: &Path, output: &Path) -> Result<ApiReport, ModelError> {
    let cancel = AtomicBool::new(false);
    let peek = api_store::read_report(output)?;
    let report: ApiReport = parse(&peek, "MODEL_API_SAVED_INVALID")?;
    let mut names: BTreeSet<String> = [
        "owner.json",
        "config.json",
        "authorization.json",
        "request.json",
        "reservation.json",
        "attempt.json",
        "report.json",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for (present, name) in [
        (report.http_request.is_some(), "http-request.json"),
        (report.observation.is_some(), "http-observation.json"),
        (report.raw_response.is_some(), "http-response.bin"),
        (report.canonical_response.is_some(), "response.json"),
        (report.candidate.is_some(), "candidate.json"),
    ] {
        if present {
            names.insert(name.into());
        }
    }
    let snapshot = api_store::ApiSnapshot::open(output, &names, &cancel)?;
    require(
        snapshot.read("report.json")? == peek,
        "MODEL_API_OUTPUT_CHANGED",
    )?;
    verify_saved(project, &report, &snapshot)?;
    snapshot.verify(&cancel)?;
    Ok(report)
}
fn verify_saved(
    project: &Path,
    report: &ApiReport,
    snapshot: &api_store::ApiSnapshot,
) -> Result<(), ModelError> {
    require(
        report.schema_version == 2
            && report.protocol == "noetome-api-single-v1"
            && matches!(report.execution_kind.as_str(), "live" | "offline_test")
            && source_model::valid_uuid(&report.request_id)
            && source_model::valid_hash(&report.request_sha256),
        "MODEL_API_SAVED_INVALID",
    )?;
    let cfg_bytes = snapshot.read("config.json")?;
    check_fingerprint(cfg_bytes, &report.configuration)?;
    let config: ApiConfiguration = parse(cfg_bytes, "MODEL_API_SAVED_INVALID")?;
    let reserved = reservation(&config)?;
    require(
        reserved == report.reservation && config.total_token_budget == report.budget_total_tokens,
        "MODEL_API_SAVED_INVALID",
    )?;
    let prepared = model_preflight::prepare_whole_text(project, &report.source.version_id)
        .map_err(|_| ModelError::from("MODEL_API_SOURCE_INVALID"))?;
    let source_files = source_guards(project, &report.source.version_id)?;
    require(
        prepared.summary() == &report.source,
        "MODEL_API_SOURCE_CHANGED",
    )?;
    let auth_bytes = snapshot.read("authorization.json")?;
    check_fingerprint(auth_bytes, &report.authorization)?;
    let auth: SourceAuthorization = parse(auth_bytes, "MODEL_API_SAVED_INVALID")?;
    authorize(
        &config,
        &report.configuration.sha256,
        &auth,
        prepared.summary(),
    )?;
    let wire = snapshot.read("request.json")?;
    require(
        sidecar::digest(wire) == report.request_sha256,
        "MODEL_API_SAVED_INVALID",
    )?;
    let saved_request: ExtractionRequest = parse(wire, "MODEL_API_SAVED_INVALID")?;
    require(
        saved_request == request(&prepared, &config, report.request_id.clone())?,
        "MODEL_API_SOURCE_CHANGED",
    )?;
    let owner = decode::strict_value(snapshot.read("owner.json")?)?;
    require(
        owner
            == serde_json::json!({"schema_version":2,"protocol":"noetome-api-single-v1","request_id":report.request_id,"request_sha256":report.request_sha256}),
        "MODEL_API_SAVED_INVALID",
    )?;
    let marker: AttemptMarker = parse(snapshot.read("attempt.json")?, "MODEL_API_SAVED_INVALID")?;
    require(
        marker.schema_version == 1
            && marker.protocol == "noetome-api-attempt-v1"
            && marker.request_id == report.request_id
            && marker.request_sha256 == report.request_sha256
            && marker.max_calls == 1
            && marker.http_state == "remote_unknown"
            && marker.execution_kind == report.execution_kind,
        "MODEL_API_SAVED_INVALID",
    )?;
    let saved_reservation: TotalTokenReservation = parse(
        snapshot.read("reservation.json")?,
        "MODEL_API_SAVED_INVALID",
    )?;
    require(
        saved_reservation == report.reservation,
        "MODEL_API_SAVED_INVALID",
    )?;
    require(
        report.attempt.pid.is_none_or(|pid| pid > 0)
            && report.attempt.elapsed_ms <= 60000
            && report.attempt.stderr_suppressed
            && report.cost_unknown
            && report.usage.cost == unknown_usage().cost
            && report.usage.provenance == UsageProvenance::ProviderReported,
        "MODEL_API_SAVED_INVALID",
    )?;
    require(
        report
            .attempt
            .failure
            .as_deref()
            .is_none_or(|code| code == "MODEL_API_TRANSPORT_FAILED"),
        "MODEL_API_SAVED_INVALID",
    )?;
    if report.attempt.failure.is_none() {
        require(
            report.attempt.status == AttemptStatus::Delivered
                && report.attempt.pid.is_some()
                && !report.attempt.stdout_suppressed,
            "MODEL_API_SAVED_INVALID",
        )?;
    } else {
        require(
            report.attempt.failure.as_deref() == Some("MODEL_API_TRANSPORT_FAILED")
                && report.attempt.stdout_suppressed,
            "MODEL_API_SAVED_INVALID",
        )?;
        if report.attempt.status == AttemptStatus::NotCalled {
            require(
                report.attempt.pid.is_none()
                    && report.observation.is_none()
                    && report.raw_response.is_none()
                    && report.canonical_response.is_none()
                    && report.candidate.is_none(),
                "MODEL_API_SAVED_INVALID",
            )?;
        }
    }
    // Paths are provenance text only. A saved report never grants new file or
    // executable capabilities; inspect operates exclusively on the held snapshot.
    for path in [
        &report.configuration_path,
        &report.authorization_path,
        &report.runtime.python_path,
        &report.runtime.adapter_path,
    ] {
        require(
            path.len() <= 4096 && !path.contains('\0'),
            "MODEL_API_SAVED_INVALID",
        )?;
    }
    // This exact reviewed adapter produced the first failed live attempt. Its
    // supported envelope discarded structural rejects; do not reinterpret those
    // receipts using the newer diagnostic raw policy or acquire their paths.
    let legacy_adapter = report.runtime.adapter.byte_length == 28025
        && report.runtime.adapter.sha256
            == "61a3f1318a7c2da869f7a2ba3fcc14db1761d84713a4da57a055bbd3aa5d31ae";
    require(
        report.runtime.schema
            == fingerprint(include_bytes!(
                "../../../sidecars/responses_api/schema.json"
            ))
            && report.runtime.instructions
                == fingerprint(include_bytes!(
                    "../../../sidecars/responses_api/instructions.txt"
                ))
            && (legacy_adapter
                || report.runtime.adapter
                    == fingerprint(include_bytes!("../../../sidecars/responses_api/adapter.py")))
            && source_model::valid_hash(&report.runtime.python.sha256)
            && report.runtime.python.byte_length > 0,
        "MODEL_API_SAVED_INVALID",
    )?;
    let body_fp = report
        .http_request
        .as_ref()
        .ok_or(ModelError::from("MODEL_API_SAVED_INVALID"))?;
    let body = snapshot.read("http-request.json")?;
    check_fingerprint(body, body_fp)?;
    decode::validate_body(body, &saved_request)?;
    let mut derived = None;
    if let Some(obs) = &report.observation {
        let stored: HttpObservation = parse(
            snapshot.read("http-observation.json")?,
            "MODEL_API_SAVED_INVALID",
        )?;
        require(stored == *obs, "MODEL_API_SAVED_INVALID")?;
        decode::validate_observation(
            obs,
            &saved_request,
            &report.request_sha256,
            &sidecar::digest(body),
        )?;
        require(
            report.http_state == obs.http_state
                && report.http_status == obs.http_status
                && report.response_id == obs.response_id
                && report.actual_model == obs.actual_model
                && report.usage.input_tokens == obs.usage_input_tokens
                && report.usage.output_tokens == obs.usage_output_tokens
                && report.usage_total_tokens == obs.usage_total_tokens,
            "MODEL_API_SAVED_INVALID",
        )?;
        require(
            report.raw_response.is_some() == obs.raw_response_available
                && report.canonical_response.is_some() == obs.canonical_response_available,
            "MODEL_API_SAVED_INVALID",
        )?;
        if obs.canonical_response_available {
            require(
                report.attempt.status == AttemptStatus::Delivered
                    && report.attempt.failure.is_none()
                    && !report.attempt.stdout_suppressed,
                "MODEL_API_SAVED_INVALID",
            )?;
        } else {
            require(
                report.failure
                    == obs
                        .failure
                        .as_deref()
                        .map(decode::failure_code)
                        .map(str::to_owned),
                "MODEL_API_SAVED_INVALID",
            )?;
        }
        if let Some(fp) = &report.raw_response {
            let bytes = snapshot.read("http-response.bin")?;
            check_fingerprint(bytes, fp)?;
            let derive = if legacy_adapter {
                decode::derive_legacy
            } else {
                decode::derive
            };
            let expected = derive(
                &saved_request,
                wire,
                body,
                bytes,
                obs.http_status
                    .ok_or(ModelError::from("MODEL_API_SAVED_INVALID"))?,
            )?;
            require(expected.1 == *obs, "MODEL_API_SAVED_INVALID")?;
            derived = expected.0;
        }
    } else {
        require(
            report.http_state == "remote_unknown"
                && report.raw_response.is_none()
                && report.canonical_response.is_none()
                && report.http_status.is_none()
                && report.response_id.is_none()
                && report.actual_model.is_none()
                && report.usage == unknown_usage()
                && report.usage_total_tokens.is_none()
                && !report.candidate_success,
            "MODEL_API_SAVED_INVALID",
        )?;
        require(
            report.failure.as_deref().is_some_and(|code| {
                matches!(
                    code,
                    "MODEL_API_REMOTE_UNKNOWN"
                        | "MODEL_API_OBSERVATION_INVALID"
                        | "MODEL_API_HTTP_BODY_INVALID"
                        | "MODEL_API_RESPONSE_INVALID"
                        | "MODEL_API_CREDENTIAL_ECHO"
                        | "MODEL_API_CREDENTIAL_INVALID"
                )
            }),
            "MODEL_API_SAVED_INVALID",
        )?;
    }
    let confirmed = if report.execution_kind == "offline_test" {
        Some(0)
    } else {
        match report.http_state.as_str() {
            "not_sent" => Some(0),
            "sent_once" => Some(1),
            "remote_unknown" => None,
            _ => return Err("MODEL_API_SAVED_INVALID".into()),
        }
    };
    require(
        report.confirmed_http_posts == confirmed,
        "MODEL_API_SAVED_INVALID",
    )?;
    if let Some(fp) = &report.canonical_response {
        let bytes = snapshot.read("response.json")?;
        check_fingerprint(bytes, fp)?;
        let outcome = model_candidate::decode_auditable_outcome(
            bytes,
            &saved_request,
            &report.request_sha256,
        )
        .map_err(|_| ModelError::from("MODEL_API_SAVED_INVALID"))?;
        require(
            derived.as_ref() == Some(&outcome) && report.usage == outcome.usage,
            "MODEL_API_SAVED_INVALID",
        )?;
        let mut evidence_request = saved_request.clone();
        evidence_request.source = prepared.evidence_source();
        let validated =
            model_candidate::decode_outcome(bytes, &evidence_request, &report.request_sha256);
        if report.candidate_success {
            let outcome = validated.map_err(|_| ModelError::from("MODEL_API_SAVED_INVALID"))?;
            require(
                outcome.status == OutcomeStatus::Complete
                    && report.failure.is_none()
                    && report.verification == "unverified"
                    && !budget_exceeded(
                        &report.usage,
                        report.usage_total_tokens,
                        report.budget_total_tokens,
                    ),
                "MODEL_API_SAVED_INVALID",
            )?;
            let fp = report
                .candidate
                .as_ref()
                .ok_or(ModelError::from("MODEL_API_SAVED_INVALID"))?;
            let bytes = snapshot.read("candidate.json")?;
            check_fingerprint(bytes, fp)?;
            let ir: CandidateIr = parse(bytes, "MODEL_API_SAVED_INVALID")?;
            require(
                report.candidate_id.as_deref() == Some(ir.candidate_id.as_str()),
                "MODEL_API_SAVED_INVALID",
            )?;
            model_candidate::validate_saved_ir(
                &ir,
                &outcome,
                &evidence_request,
                &report.request_sha256,
                &report.canonical_response.as_ref().unwrap().sha256,
            )
            .map_err(|_| ModelError::from("MODEL_API_SAVED_INVALID"))?;
        } else {
            require(
                validated.is_err()
                    || budget_exceeded(
                        &report.usage,
                        report.usage_total_tokens,
                        report.budget_total_tokens,
                    ),
                "MODEL_API_SAVED_INVALID",
            )?;
            let failure = if budget_exceeded(
                &report.usage,
                report.usage_total_tokens,
                report.budget_total_tokens,
            ) {
                "MODEL_API_USAGE_BUDGET_EXCEEDED"
            } else {
                "MODEL_API_EVIDENCE_INVALID"
            };
            require(
                report.failure.as_deref() == Some(failure),
                "MODEL_API_SAVED_INVALID",
            )?;
        }
    } else {
        require(!report.candidate_success, "MODEL_API_SAVED_INVALID")?;
    }
    if !report.candidate_success {
        require(
            report.candidate.is_none()
                && report.candidate_id.is_none()
                && report.verification == "not_applicable"
                && report
                    .failure
                    .as_deref()
                    .is_some_and(decode::allowed_failure),
            "MODEL_API_SAVED_INVALID",
        )?;
    }
    prepared
        .verify()
        .map_err(|_| ModelError::from("MODEL_API_SOURCE_CHANGED"))?;
    for file in source_files {
        file.verify()?;
    }
    snapshot.verify(&AtomicBool::new(false))
}

#[cfg(test)]
fn extract_for_test(
    project: &Path,
    version: &str,
    config: &Path,
    authorization: &Path,
    output: &Path,
    python: &Path,
    credential: &TransientCredential,
    invoke: impl FnOnce(&ExtractionRequest, &[u8], &Path, &TransientCredential) -> SensitiveAttempt,
) -> Result<ApiReport, ModelError> {
    let admission = Admission::open(project, version, config, authorization, python)?;
    api_store::preflight(project, output)?;
    run_admitted(admission, output, credential, "offline_test", invoke)
}
