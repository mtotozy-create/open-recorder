use std::{sync::mpsc, thread, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use chrono::Utc;
use futures_util::{Sink, SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use reqwest::{
    blocking::Client,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, DATE},
    Method,
};
use serde_json::{json, Value};
use sha1::Sha1;
use tokio::sync::mpsc as tokio_mpsc;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use uuid::Uuid;

type HmacSha1 = Hmac<Sha1>;

const ALIYUN_API_VERSION: &str = "2023-09-30";
const ALIYUN_SIGNATURE_METHOD: &str = "HMAC-SHA1";
const ALIYUN_SIGNATURE_VERSION: &str = "1.0";
const TINGWU_WS_NAMESPACE: &str = "SpeechTranscriber";
const TINGWU_WS_APPKEY_DEFAULT: &str = "default";
const TINGWU_WS_AUDIO_CHUNK_BYTES_16K: usize = 3200;
const REALTIME_RECONNECT_DELAY: Duration = Duration::from_secs(5);
const REALTIME_RECONNECT_MAX_RETRIES: usize = 3;

#[derive(Debug, Clone)]
pub struct AliyunTingwuRealtimeConfig {
    pub access_key_id: String,
    pub access_key_secret: String,
    pub app_key: String,
    pub endpoint: String,
    pub format: String,
    pub sample_rate: u32,
    pub source_language: String,
    pub language_hints: Vec<String>,
    pub task_key: Option<String>,
    pub progressive_callbacks_enabled: bool,
    pub transcoding_target_audio_format: Option<String>,
    pub transcription_output_level: u8,
    pub transcription_diarization_enabled: bool,
    pub transcription_diarization_speaker_count: Option<u32>,
    pub transcription_phrase_id: Option<String>,
    pub translation_enabled: bool,
    pub translation_output_level: u8,
    pub translation_target_languages: Vec<String>,
    pub auto_chapters_enabled: bool,
    pub meeting_assistance_enabled: bool,
    pub summarization_enabled: bool,
    pub summarization_types: Vec<String>,
    pub text_polish_enabled: bool,
    pub service_inspection_enabled: bool,
    pub service_inspection: Option<Value>,
    pub custom_prompt_enabled: bool,
    pub custom_prompt: Option<Value>,
}

#[derive(Debug, Clone, Copy)]
pub enum RealtimeWorkerState {
    Idle,
    Connecting,
    Running,
    Paused,
    Stopping,
    Error,
}

#[derive(Debug, Clone)]
pub enum RealtimeWorkerEvent {
    StateChanged {
        state: RealtimeWorkerState,
        error: Option<String>,
    },
    FinalSentence {
        text: String,
        event_time_ms: Option<u64>,
        sentence_id: Option<String>,
        sentence_index: Option<u64>,
    },
    TranslatedSentence {
        text: String,
        event_time_ms: Option<u64>,
        source_sentence_id: Option<String>,
        sentence_index: Option<u64>,
        target_language: Option<String>,
    },
}

#[derive(Debug)]
pub enum RealtimeWorkerCommand {
    AudioFrame(Vec<i16>),
    Pause,
    Resume,
    Stop,
}

pub struct RealtimeWorkerHandle {
    command_tx: tokio_mpsc::Sender<RealtimeWorkerCommand>,
    join_handle: Option<thread::JoinHandle<()>>,
}

impl RealtimeWorkerHandle {
    pub fn push_audio_frame(&self, frame: Vec<i16>) {
        let _ = self.command_tx.try_send(RealtimeWorkerCommand::AudioFrame(frame));
    }

    pub fn pause(&self) -> Result<(), String> {
        self.command_tx
            .try_send(RealtimeWorkerCommand::Pause)
            .map_err(|error| format!("failed to pause realtime stream: {error}"))
    }

    pub fn resume(&self) -> Result<(), String> {
        self.command_tx
            .try_send(RealtimeWorkerCommand::Resume)
            .map_err(|error| format!("failed to resume realtime stream: {error}"))
    }

    pub fn stop(mut self) -> Result<(), String> {
        self.command_tx
            .try_send(RealtimeWorkerCommand::Stop)
            .map_err(|error| format!("failed to stop realtime stream: {error}"))?;
        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
        Ok(())
    }
}

pub fn start_realtime_worker(
    config: AliyunTingwuRealtimeConfig,
    event_tx: mpsc::Sender<RealtimeWorkerEvent>,
) -> RealtimeWorkerHandle {
    let (command_tx, command_rx) = tokio_mpsc::channel::<RealtimeWorkerCommand>(64);

    let join_handle = thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = event_tx.send(RealtimeWorkerEvent::StateChanged {
                    state: RealtimeWorkerState::Error,
                    error: Some(format!("failed to create async runtime: {error}")),
                });
                return;
            }
        };

        let result = runtime.block_on(run_realtime_worker(config, command_rx, event_tx.clone()));
        if let Err(error) = result {
            let _ = event_tx.send(RealtimeWorkerEvent::StateChanged {
                state: RealtimeWorkerState::Error,
                error: Some(error),
            });
            let _ = event_tx.send(RealtimeWorkerEvent::StateChanged {
                state: RealtimeWorkerState::Idle,
                error: None,
            });
        }
    });

    RealtimeWorkerHandle {
        command_tx,
        join_handle: Some(join_handle),
    }
}

fn realtime_audio_chunk_bytes(sample_rate: u32) -> usize {
    if sample_rate == 8000 {
        1600
    } else {
        TINGWU_WS_AUDIO_CHUNK_BYTES_16K
    }
}

type RealtimeWsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type RealtimeWsWriter = futures_util::stream::SplitSink<RealtimeWsStream, Message>;
type RealtimeWsReader = futures_util::stream::SplitStream<RealtimeWsStream>;

enum WaitReconnectOutcome {
    Continue,
    StopRequested,
}

enum RealtimeSessionOutcome {
    StopRequested,
    RetryableDisconnect(String),
    Fatal(String),
}

fn emit_state(event_tx: &mpsc::Sender<RealtimeWorkerEvent>, state: RealtimeWorkerState) {
    let _ = event_tx.send(RealtimeWorkerEvent::StateChanged { state, error: None });
}

fn apply_disconnected_command(
    command: RealtimeWorkerCommand,
    paused: &mut bool,
    event_tx: &mpsc::Sender<RealtimeWorkerEvent>,
) -> bool {
    match command {
        RealtimeWorkerCommand::AudioFrame(_) => false,
        RealtimeWorkerCommand::Pause => {
            if !*paused {
                *paused = true;
                emit_state(event_tx, RealtimeWorkerState::Paused);
            }
            false
        }
        RealtimeWorkerCommand::Resume => {
            if *paused {
                *paused = false;
                emit_state(event_tx, RealtimeWorkerState::Connecting);
            }
            false
        }
        RealtimeWorkerCommand::Stop => {
            emit_state(event_tx, RealtimeWorkerState::Stopping);
            true
        }
    }
}

fn drain_disconnected_commands(
    command_rx: &mut tokio_mpsc::Receiver<RealtimeWorkerCommand>,
    paused: &mut bool,
    event_tx: &mpsc::Sender<RealtimeWorkerEvent>,
) -> bool {
    loop {
        match command_rx.try_recv() {
            Ok(command) => {
                if apply_disconnected_command(command, paused, event_tx) {
                    return true;
                }
            }
            Err(tokio_mpsc::error::TryRecvError::Empty) => return false,
            Err(tokio_mpsc::error::TryRecvError::Disconnected) => {
                emit_state(event_tx, RealtimeWorkerState::Stopping);
                return true;
            }
        }
    }
}

async fn wait_for_reconnect_or_stop(
    command_rx: &mut tokio_mpsc::Receiver<RealtimeWorkerCommand>,
    paused: &mut bool,
    event_tx: &mpsc::Sender<RealtimeWorkerEvent>,
) -> WaitReconnectOutcome {
    let delay = tokio::time::sleep(REALTIME_RECONNECT_DELAY);
    tokio::pin!(delay);

    loop {
        tokio::select! {
            _ = &mut delay => {
                return WaitReconnectOutcome::Continue;
            }
            maybe_command = command_rx.recv() => {
                match maybe_command {
                    Some(command) => {
                        if apply_disconnected_command(command, paused, event_tx) {
                            return WaitReconnectOutcome::StopRequested;
                        }
                    }
                    None => {
                        emit_state(event_tx, RealtimeWorkerState::Stopping);
                        return WaitReconnectOutcome::StopRequested;
                    }
                }
            }
        }
    }
}

async fn close_realtime_connection(
    writer: &mut RealtimeWsWriter,
    config: &AliyunTingwuRealtimeConfig,
    task_id: &str,
    emit_stop_error: bool,
    event_tx: &mpsc::Sender<RealtimeWorkerEvent>,
) {
    let _ = writer.close().await;
    let stop_config = config.clone();
    let stop_task_id = task_id.to_string();
    let stop_result =
        tokio::task::spawn_blocking(move || stop_realtime_task(&stop_config, &stop_task_id))
            .await;

    if !emit_stop_error {
        return;
    }

    let stop_error = match stop_result {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(error) => Some(format!("failed to join realtime stop task: {error}")),
    };
    if let Some(error) = stop_error {
        let _ = event_tx.send(RealtimeWorkerEvent::StateChanged {
            state: RealtimeWorkerState::Error,
            error: Some(error),
        });
    }
}

async fn establish_realtime_connection(
    config: &AliyunTingwuRealtimeConfig,
    event_tx: &mpsc::Sender<RealtimeWorkerEvent>,
    paused: bool,
) -> Result<(String, String, RealtimeWsWriter, RealtimeWsReader, bool), String> {
    let create_config = config.clone();
    let create_result = tokio::task::spawn_blocking(move || create_realtime_task(&create_config))
        .await
        .map_err(|error| format!("failed to join realtime create task: {error}"))?;
    let (task_id, meeting_join_url) = create_result?;

    let (websocket, _) = connect_async(meeting_join_url)
        .await
        .map_err(|error| format!("failed to connect realtime websocket: {error}"))?;
    let (mut writer, reader) = websocket.split();
    let stream_task_id = Uuid::new_v4().simple().to_string();
    let transcription_started = false;

    if paused {
        emit_state(event_tx, RealtimeWorkerState::Paused);
    } else {
        send_start_transcription(
            &mut writer,
            &stream_task_id,
            &config.format,
            config.sample_rate,
            config.transcription_output_level,
            TINGWU_WS_APPKEY_DEFAULT,
        )
        .await?;
    }

    Ok((
        task_id,
        stream_task_id,
        writer,
        reader,
        transcription_started,
    ))
}

async fn run_connected_session(
    config: &AliyunTingwuRealtimeConfig,
    event_tx: &mpsc::Sender<RealtimeWorkerEvent>,
    command_rx: &mut tokio_mpsc::Receiver<RealtimeWorkerCommand>,
    writer: &mut RealtimeWsWriter,
    reader: &mut RealtimeWsReader,
    stream_task_id: &str,
    paused: &mut bool,
    transcription_started: &mut bool,
    audio_chunk_bytes: usize,
) -> RealtimeSessionOutcome {
    let mut last_server_text: Option<String> = None;

    loop {
        tokio::select! {
            maybe_command = command_rx.recv() => {
                match maybe_command {
                    Some(RealtimeWorkerCommand::AudioFrame(frame)) => {
                        if !*paused && *transcription_started && !frame.is_empty() {
                            let bytes = pcm16_to_le_bytes(&frame);
                            for chunk in bytes.chunks(audio_chunk_bytes) {
                                if let Err(error) = writer.send(Message::Binary(chunk.to_vec().into())).await {
                                    return RealtimeSessionOutcome::RetryableDisconnect(
                                        format!("failed to send realtime audio frame: {error}")
                                    );
                                }
                            }
                        }
                    }
                    Some(RealtimeWorkerCommand::Pause) => {
                        if !*paused {
                            if let Err(error) = send_stop_transcription(
                                writer,
                                stream_task_id,
                                TINGWU_WS_APPKEY_DEFAULT,
                            ).await {
                                return RealtimeSessionOutcome::RetryableDisconnect(error);
                            }
                            *paused = true;
                            *transcription_started = false;
                            emit_state(event_tx, RealtimeWorkerState::Paused);
                        }
                    }
                    Some(RealtimeWorkerCommand::Resume) => {
                        if *paused {
                            if let Err(error) = send_start_transcription(
                                writer,
                                stream_task_id,
                                &config.format,
                                config.sample_rate,
                                config.transcription_output_level,
                                TINGWU_WS_APPKEY_DEFAULT,
                            ).await {
                                return RealtimeSessionOutcome::RetryableDisconnect(error);
                            }
                            *paused = false;
                            *transcription_started = false;
                            emit_state(event_tx, RealtimeWorkerState::Connecting);
                        }
                    }
                    Some(RealtimeWorkerCommand::Stop) => {
                        emit_state(event_tx, RealtimeWorkerState::Stopping);
                        if !*paused {
                            let _ = send_stop_transcription(
                                writer,
                                stream_task_id,
                                TINGWU_WS_APPKEY_DEFAULT,
                            ).await;
                        }
                        return RealtimeSessionOutcome::StopRequested;
                    }
                    None => {
                        emit_state(event_tx, RealtimeWorkerState::Stopping);
                        if !*paused {
                            let _ = send_stop_transcription(
                                writer,
                                stream_task_id,
                                TINGWU_WS_APPKEY_DEFAULT,
                            ).await;
                        }
                        return RealtimeSessionOutcome::StopRequested;
                    }
                }
            }
            maybe_message = reader.next() => {
                match maybe_message {
                    Some(Ok(Message::Text(raw_text))) => {
                        last_server_text = Some(raw_text.to_string());
                        if let Some(error) = extract_ws_error_message(raw_text.as_str()) {
                            return RealtimeSessionOutcome::Fatal(format!("realtime service error: {error}"));
                        }
                        let name = extract_ws_event_name(raw_text.as_str());
                        if name.as_deref() == Some("transcriptionstarted") {
                            *transcription_started = true;
                            emit_state(event_tx, RealtimeWorkerState::Running);
                        }
                        if name.as_deref() == Some("transcriptioncompleted") {
                            *transcription_started = false;
                        }
                        handle_ws_text_message(raw_text.as_str(), event_tx);
                    }
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Close(frame))) => {
                        return RealtimeSessionOutcome::RetryableDisconnect(format!(
                            "realtime websocket closed unexpectedly{}{}{}",
                            close_frame_reason(frame.as_ref()),
                            if *transcription_started { "" } else { ", before transcription started ack" },
                            last_server_text_suffix(last_server_text.as_deref())
                        ));
                    }
                    Some(Err(error)) => {
                        return RealtimeSessionOutcome::RetryableDisconnect(format!(
                            "realtime websocket read error: {error}"
                        ));
                    }
                    None => {
                        return RealtimeSessionOutcome::RetryableDisconnect(format!(
                            "realtime websocket closed unexpectedly by peer{}{}",
                            if *transcription_started { "" } else { ", before transcription started ack" },
                            last_server_text_suffix(last_server_text.as_deref())
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn run_realtime_worker(
    config: AliyunTingwuRealtimeConfig,
    mut command_rx: tokio_mpsc::Receiver<RealtimeWorkerCommand>,
    event_tx: mpsc::Sender<RealtimeWorkerEvent>,
) -> Result<(), String> {
    let mut paused = false;
    let mut retry_attempts = 0usize;
    let audio_chunk_bytes = realtime_audio_chunk_bytes(config.sample_rate);

    loop {
        if drain_disconnected_commands(&mut command_rx, &mut paused, &event_tx) {
            break;
        }

        emit_state(
            &event_tx,
            if paused {
                RealtimeWorkerState::Paused
            } else {
                RealtimeWorkerState::Connecting
            },
        );

        let connection = establish_realtime_connection(&config, &event_tx, paused).await;
        let (task_id, stream_task_id, mut writer, mut reader, mut transcription_started) =
            match connection {
                Ok(items) => {
                    retry_attempts = 0;
                    items
                }
                Err(error) => {
                    if retry_attempts == 0 {
                        return Err(error);
                    }
                    retry_attempts += 1;
                    if retry_attempts > REALTIME_RECONNECT_MAX_RETRIES {
                        return Err(format!(
                            "realtime websocket disconnected; retried every {} seconds for {} times but still failed: {}",
                            REALTIME_RECONNECT_DELAY.as_secs(),
                            REALTIME_RECONNECT_MAX_RETRIES,
                            error
                        ));
                    }
                    emit_state(
                        &event_tx,
                        if paused {
                            RealtimeWorkerState::Paused
                        } else {
                            RealtimeWorkerState::Connecting
                        },
                    );
                    if matches!(
                        wait_for_reconnect_or_stop(&mut command_rx, &mut paused, &event_tx).await,
                        WaitReconnectOutcome::StopRequested
                    ) {
                        break;
                    }
                    continue;
                }
            };

        let outcome = run_connected_session(
            &config,
            &event_tx,
            &mut command_rx,
            &mut writer,
            &mut reader,
            &stream_task_id,
            &mut paused,
            &mut transcription_started,
            audio_chunk_bytes,
        )
        .await;

        let emit_stop_error = matches!(outcome, RealtimeSessionOutcome::StopRequested);
        close_realtime_connection(&mut writer, &config, &task_id, emit_stop_error, &event_tx).await;

        match outcome {
            RealtimeSessionOutcome::StopRequested => break,
            RealtimeSessionOutcome::Fatal(error) => return Err(error),
            RealtimeSessionOutcome::RetryableDisconnect(error) => {
                retry_attempts += 1;
                if retry_attempts > REALTIME_RECONNECT_MAX_RETRIES {
                    return Err(format!(
                        "realtime websocket disconnected; retried every {} seconds for {} times but still failed: {}",
                        REALTIME_RECONNECT_DELAY.as_secs(),
                        REALTIME_RECONNECT_MAX_RETRIES,
                        error
                    ));
                }
                emit_state(
                    &event_tx,
                    if paused {
                        RealtimeWorkerState::Paused
                    } else {
                        RealtimeWorkerState::Connecting
                    },
                );
                if matches!(
                    wait_for_reconnect_or_stop(&mut command_rx, &mut paused, &event_tx).await,
                    WaitReconnectOutcome::StopRequested
                ) {
                    break;
                }
            }
        }
    }
    emit_state(&event_tx, RealtimeWorkerState::Idle);
    Ok(())
}

fn pcm16_to_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn send_signed_json_request(
    client: &Client,
    method: Method,
    url: &str,
    canonicalized_resource: &str,
    body: Option<&Value>,
    config: &AliyunTingwuRealtimeConfig,
) -> Result<Value, String> {
    let accept = "application/json";
    let body_text = match body {
        Some(value) => Some(
            serde_json::to_string(value)
                .map_err(|error| format!("failed to serialize request body: {error}"))?,
        ),
        None => None,
    };
    let content_type = if body_text.is_some() {
        "application/json; charset=utf-8"
    } else {
        ""
    };
    let content_md5 = body_text
        .as_ref()
        .map(|value| BASE64_STANDARD.encode(md5::compute(value.as_bytes()).0))
        .unwrap_or_default();
    let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let nonce = Uuid::new_v4().to_string();

    let signature_headers = vec![
        (
            "x-acs-signature-method".to_string(),
            ALIYUN_SIGNATURE_METHOD.to_string(),
        ),
        (
            "x-acs-signature-version".to_string(),
            ALIYUN_SIGNATURE_VERSION.to_string(),
        ),
        ("x-acs-signature-nonce".to_string(), nonce.clone()),
        ("x-acs-version".to_string(), ALIYUN_API_VERSION.to_string()),
    ];
    let canonicalized_headers = build_canonicalized_headers(&signature_headers);

    let string_to_sign = format!(
        "{}\n{}\n{}\n{}\n{}\n{}{}",
        method.as_str(),
        accept,
        content_md5,
        content_type,
        date,
        canonicalized_headers,
        canonicalized_resource
    );

    let mut mac = HmacSha1::new_from_slice(config.access_key_secret.as_bytes())
        .map_err(|error| format!("failed to initialize HMAC: {error}"))?;
    mac.update(string_to_sign.as_bytes());
    let signature = BASE64_STANDARD.encode(mac.finalize().into_bytes());
    let authorization = format!("acs {}:{signature}", config.access_key_id);

    let mut request_builder = client
        .request(method, url)
        .header(ACCEPT, accept)
        .header(DATE, date)
        .header(AUTHORIZATION, authorization)
        .header("x-acs-signature-method", ALIYUN_SIGNATURE_METHOD)
        .header("x-acs-signature-version", ALIYUN_SIGNATURE_VERSION)
        .header("x-acs-signature-nonce", nonce)
        .header("x-acs-version", ALIYUN_API_VERSION);

    if !content_type.is_empty() {
        request_builder = request_builder.header(CONTENT_TYPE, content_type);
    }
    if !content_md5.is_empty() {
        request_builder = request_builder.header("Content-MD5", content_md5);
    }
    if let Some(body_text) = body_text {
        request_builder = request_builder.body(body_text);
    }

    let response = request_builder
        .send()
        .map_err(|error| format!("request failed: {error}"))?;
    let status = response.status();
    let body_text = response
        .text()
        .map_err(|error| format!("failed to read response body: {error}"))?;
    if !status.is_success() {
        return Err(format!("request failed with {status}: {body_text}"));
    }

    serde_json::from_str::<Value>(&body_text)
        .map_err(|error| format!("failed to parse JSON response: {error}; body={body_text}"))
}

fn create_realtime_task(config: &AliyunTingwuRealtimeConfig) -> Result<(String, String), String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("failed to create http client: {error}"))?;

    let endpoint = config.endpoint.trim_end_matches('/');
    let path = "/openapi/tingwu/v2/tasks";
    let query = "type=realtime";
    let url = format!("{endpoint}{path}?{query}");
    let resource = format!("{path}?{query}");

    let mut input = json!({
        "Format": config.format.as_str(),
        "SampleRate": config.sample_rate,
        "SourceLanguage": config.source_language.as_str(),
        "TaskKey": config
            .task_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.to_string())
            .unwrap_or_else(|| format!("open-recorder-realtime-{}", Uuid::new_v4())),
    });
    if !config.language_hints.is_empty() {
        input["LanguageHints"] = json!(config.language_hints.clone());
    }
    if config.progressive_callbacks_enabled {
        input["ProgressiveCallbacksEnabled"] = json!(true);
    }

    let mut transcription = json!({
        "OutputLevel": config.transcription_output_level.clamp(1, 2)
    });
    if config.transcription_diarization_enabled {
        transcription["DiarizationEnabled"] = json!(true);
        if let Some(speaker_count) = config.transcription_diarization_speaker_count {
            transcription["Diarization"] = json!({
                "SpeakerCount": speaker_count
            });
        }
    }
    if let Some(phrase_id) = config
        .transcription_phrase_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        transcription["PhraseId"] = json!(phrase_id);
    }

    let mut parameters = json!({
        "Transcription": transcription
    });
    if let Some(target_audio_format) = config
        .transcoding_target_audio_format
        .as_deref()
        .map(str::trim)
        .filter(|value| value.eq_ignore_ascii_case("mp3"))
    {
        parameters["Transcoding"] = json!({
            "TargetAudioFormat": target_audio_format.to_ascii_lowercase()
        });
    }
    if config.translation_enabled && !config.translation_target_languages.is_empty() {
        parameters["TranslationEnabled"] = json!(true);
        parameters["Translation"] = json!({
            "OutputLevel": config.translation_output_level.clamp(1, 2),
            "TargetLanguages": config.translation_target_languages.clone()
        });
    }
    if config.auto_chapters_enabled {
        parameters["AutoChaptersEnabled"] = json!(true);
    }
    if config.meeting_assistance_enabled {
        parameters["MeetingAssistanceEnabled"] = json!(true);
    }
    if config.summarization_enabled {
        parameters["SummarizationEnabled"] = json!(true);
        if !config.summarization_types.is_empty() {
            parameters["Summarization"] = json!({
                "Types": config.summarization_types.clone()
            });
        }
    }
    if config.text_polish_enabled {
        parameters["TextPolishEnabled"] = json!(true);
    }
    if config.service_inspection_enabled {
        parameters["ServiceInspectionEnabled"] = json!(true);
        if let Some(value) = config.service_inspection.clone() {
            parameters["ServiceInspection"] = value;
        }
    }
    if config.custom_prompt_enabled {
        parameters["CustomPromptEnabled"] = json!(true);
        if let Some(value) = config.custom_prompt.clone() {
            parameters["CustomPrompt"] = value;
        }
    }

    let body = json!({
        "AppKey": config.app_key.as_str(),
        "Input": input,
        "Parameters": parameters
    });

    let payload = send_signed_json_request(
        &client,
        Method::PUT,
        &url,
        &resource,
        Some(&body),
        config,
    )?;
    let task_id = extract_string(
        &payload,
        &[
            "/Data/TaskId",
            "/Data/taskId",
            "/TaskId",
            "/taskId",
            "/data/taskId",
        ],
    )
    .ok_or_else(|| format!("realtime create response missing TaskId: {payload}"))?;
    let meeting_join_url = extract_string(
        &payload,
        &[
            "/Data/MeetingJoinUrl",
            "/Data/meetingJoinUrl",
            "/meetingJoinUrl",
            "/MeetingJoinUrl",
        ],
    )
    .ok_or_else(|| format!("realtime create response missing MeetingJoinUrl: {payload}"))?;
    Ok((task_id, meeting_join_url))
}

fn stop_realtime_task(config: &AliyunTingwuRealtimeConfig, task_id: &str) -> Result<(), String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("failed to create http client: {error}"))?;

    let endpoint = config.endpoint.trim_end_matches('/');
    let path = "/openapi/tingwu/v2/tasks";
    // Aliyun signature verification uses canonicalized query ordering (lexicographic by key).
    // Keep the query order aligned with server-side canonical string to sign.
    let query = "operation=stop&type=realtime";
    let url = format!("{endpoint}{path}?{query}");
    let resource = format!("{path}?{query}");
    let body = json!({
        "Input": {
            "TaskId": task_id
        }
    });

    let _ = send_signed_json_request(
        &client,
        Method::PUT,
        &url,
        &resource,
        Some(&body),
        config,
    )?;
    Ok(())
}

fn build_canonicalized_headers(headers: &[(String, String)]) -> String {
    let mut entries = headers.to_vec();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
        .iter()
        .map(|(key, value)| format!("{}:{}\n", key.to_lowercase(), value.trim()))
        .collect::<String>()
}

fn extract_string(payload: &Value, pointers: &[&str]) -> Option<String> {
    pointers.iter().find_map(|pointer| {
        payload
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

async fn send_start_transcription<S>(
    writer: &mut S,
    task_id: &str,
    format: &str,
    sample_rate: u32,
    output_level: u8,
    appkey: &str,
) -> Result<(), String>
where
    S: Sink<Message> + Unpin,
    <S as Sink<Message>>::Error: std::fmt::Display,
{
    let command = json!({
        "header": {
            "message_id": Uuid::new_v4().simple().to_string(),
            "task_id": task_id,
            "namespace": TINGWU_WS_NAMESPACE,
            "name": "StartTranscription",
            "appkey": appkey
        },
        "payload": {
            "format": format,
            "sample_rate": sample_rate,
            "enable_intermediate_result": output_level.clamp(1, 2) == 2
        }
    });
    let raw = serde_json::to_string(&command)
        .map_err(|error| format!("failed to serialize start command: {error}"))?;
    writer
        .send(Message::Text(raw.into()))
        .await
        .map_err(|error| format!("failed to send start command: {error}"))
}

async fn send_stop_transcription<S>(writer: &mut S, task_id: &str, appkey: &str) -> Result<(), String>
where
    S: Sink<Message> + Unpin,
    <S as Sink<Message>>::Error: std::fmt::Display,
{
    let command = json!({
        "header": {
            "message_id": Uuid::new_v4().simple().to_string(),
            "task_id": task_id,
            "namespace": TINGWU_WS_NAMESPACE,
            "name": "StopTranscription",
            "appkey": appkey
        },
        "payload": {}
    });
    let raw = serde_json::to_string(&command)
        .map_err(|error| format!("failed to serialize stop command: {error}"))?;
    writer
        .send(Message::Text(raw.into()))
        .await
        .map_err(|error| format!("failed to send stop command: {error}"))
}

fn handle_ws_text_message(raw_text: &str, event_tx: &mpsc::Sender<RealtimeWorkerEvent>) {
    let payload = match serde_json::from_str::<Value>(raw_text) {
        Ok(value) => value,
        Err(_) => return,
    };

    let name = extract_ws_event_name_from_payload(&payload).unwrap_or_default();

    if name == "taskfailed" {
        let message = extract_string(
            &payload,
            &[
                "/payload/message",
                "/payload/error_message",
                "/payload/result",
                "/payload/text",
            ],
        )
        .unwrap_or_else(|| "realtime task failed".to_string());
        let _ = event_tx.send(RealtimeWorkerEvent::StateChanged {
            state: RealtimeWorkerState::Error,
            error: Some(message),
        });
        return;
    }

    if name == "resulttranslated" || name == "translationresultchanged" {
        let translation_text = extract_translation_text(&payload);
        let Some(translation_text) = translation_text else {
            return;
        };
        if translation_text.trim().is_empty() {
            return;
        }
        let event_time_ms = extract_translation_end_time(&payload).or_else(|| {
            extract_u64(
                &payload,
                &[
                    "/payload/time",
                    "/payload/end_time",
                    "/payload/endTime",
                    "/payload/output/transcription/endTime",
                ],
            )
        });
        let source_sentence_id = extract_string(
            &payload,
            &[
                "/header/source_message_id",
                "/header/sourceMessageId",
                "/payload/source_message_id",
                "/payload/sourceMessageId",
                "/payload/translation_result/0/source_message_id",
                "/payload/translationResult/0/sourceMessageId",
                "/payload/translate_result/0/source_message_id",
                "/payload/translateResult/0/sourceMessageId",
            ],
        );
        let sentence_index = extract_translation_index(&payload).or_else(|| {
            extract_u64(
                &payload,
                &[
                    "/payload/translation_result/0/source_sentence_index",
                    "/payload/translationResult/0/sourceSentenceIndex",
                    "/payload/source_sentence_index",
                    "/payload/sourceSentenceIndex",
                ],
            )
        });
        let target_language = extract_string(
            &payload,
            &[
                "/payload/translation_result/0/target_lang",
                "/payload/translationResult/0/targetLang",
                "/payload/translate_result/0/target_lang",
                "/payload/translateResult/0/targetLang",
                "/payload/translation_result/0/target_lang",
                "/payload/translationResult/0/targetLang",
                "/payload/translation_result/0/target_language",
                "/payload/translationResult/0/targetLanguage",
                "/payload/target_lang",
                "/payload/targetLang",
            ],
        );
        let _ = event_tx.send(RealtimeWorkerEvent::TranslatedSentence {
            text: translation_text,
            event_time_ms,
            source_sentence_id,
            sentence_index,
            target_language,
        });
        return;
    }

    let is_sentence_end = name == "sentenceend"
        || (name == "transcriptionresultchanged"
            && payload
                .pointer("/payload/sentence_end")
                .and_then(Value::as_bool)
                .unwrap_or(false));
    if !is_sentence_end {
        return;
    }

    let text = extract_string(
        &payload,
        &[
            "/payload/result",
            "/payload/text",
            "/payload/transSentenceText",
            "/payload/sentence/text",
            "/payload/transcription/text",
            "/payload/output/transcription/text",
        ],
    );
    let Some(text) = text else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }

    let event_time_ms = extract_u64(
        &payload,
        &[
            "/payload/time",
            "/payload/output/transcription/endTime",
            "/payload/end_time",
            "/payload/endTime",
        ],
    );
    let sentence_id = extract_string(&payload, &["/header/message_id", "/header/messageId"]);
    let sentence_index = extract_u64(
        &payload,
        &[
            "/payload/sentence_index",
            "/payload/sentenceIndex",
            "/payload/index",
            "/payload/output/transcription/sentenceIndex",
        ],
    );

    let _ = event_tx.send(RealtimeWorkerEvent::FinalSentence {
        text,
        event_time_ms,
        sentence_id,
        sentence_index,
    });
}

fn extract_u64(payload: &Value, pointers: &[&str]) -> Option<u64> {
    pointers
        .iter()
        .find_map(|pointer| payload.pointer(pointer).and_then(Value::as_u64))
}

fn extract_translation_entries(payload: &Value) -> Vec<&Value> {
    let pointers = [
        "/payload/translate_result",
        "/payload/translateResult",
        "/payload/translation_result",
        "/payload/translationResult",
    ];
    pointers
        .iter()
        .find_map(|pointer| payload.pointer(pointer).and_then(Value::as_array))
        .map(|items| items.iter().collect::<Vec<_>>())
        .unwrap_or_default()
}

fn pick_translation_entry<'a>(payload: &'a Value) -> Option<&'a Value> {
    let entries = extract_translation_entries(payload);
    if entries.is_empty() {
        return None;
    }
    if let Some(item) = entries
        .iter()
        .rev()
        .find(|item| !item.pointer("/partial").and_then(Value::as_bool).unwrap_or(false))
    {
        return Some(item);
    }
    entries.last().copied()
}

fn extract_translation_text(payload: &Value) -> Option<String> {
    if let Some(entry) = pick_translation_entry(payload) {
        if let Some(text) = extract_string(entry, &["/text", "/translation_text", "/translationText"])
        {
            return Some(text);
        }
    }
    extract_string(
        payload,
        &[
            "/payload/translation_text",
            "/payload/translationText",
            "/payload/result",
            "/payload/text",
        ],
    )
}

fn extract_translation_index(payload: &Value) -> Option<u64> {
    if let Some(entry) = pick_translation_entry(payload) {
        if let Some(index) = extract_u64(entry, &["/index"]) {
            return Some(index);
        }
    }
    None
}

fn extract_translation_end_time(payload: &Value) -> Option<u64> {
    if let Some(entry) = pick_translation_entry(payload) {
        if let Some(end_time) = extract_u64(entry, &["/endTime", "/end_time"]) {
            return Some(end_time);
        }
    }
    None
}

fn extract_ws_event_name(raw_text: &str) -> Option<String> {
    let payload = serde_json::from_str::<Value>(raw_text).ok()?;
    extract_ws_event_name_from_payload(&payload)
}

fn extract_ws_event_name_from_payload(payload: &Value) -> Option<String> {
    extract_string(
        payload,
        &[
            "/header/name",
            "/header/event",
            "/header/action",
            "/event",
        ],
    )
    .map(|value| value.to_ascii_lowercase())
}

fn close_frame_reason(
    frame: Option<&tokio_tungstenite::tungstenite::protocol::CloseFrame>,
) -> String {
    if let Some(value) = frame {
        let reason = value.reason.to_string();
        if reason.trim().is_empty() {
            return format!(" (code={})", value.code);
        }
        return format!(" (code={}, reason={reason})", value.code);
    }
    String::new()
}

fn last_server_text_suffix(last_text: Option<&str>) -> String {
    let Some(raw) = last_text else {
        return String::new();
    };
    let snippet = raw.trim();
    if snippet.is_empty() {
        return String::new();
    }
    let shortened = if snippet.len() > 280 {
        format!("{}...", &snippet[..280])
    } else {
        snippet.to_string()
    };
    format!(", last_server_text={shortened}")
}

fn extract_ws_error_message(raw_text: &str) -> Option<String> {
    let payload = serde_json::from_str::<Value>(raw_text).ok()?;
    let code_value = payload
        .pointer("/header/status")
        .and_then(Value::as_i64)
        .or_else(|| payload.pointer("/header/code").and_then(Value::as_i64))
        .or_else(|| payload.pointer("/code").and_then(Value::as_i64));
    let message = extract_string(
        &payload,
        &[
            "/header/message",
            "/header/error_message",
            "/payload/message",
            "/payload/error_message",
            "/message",
            "/errorMessage",
        ],
    );

    match (code_value, message) {
        (Some(code), Some(msg)) if code != 0 && code != 200 && code != 20_000_000 => {
            Some(format!("code={code}, {msg}"))
        }
        (Some(code), None) if code != 0 && code != 200 && code != 20_000_000 => {
            Some(format!("code={code}"))
        }
        (_, Some(msg)) => {
            let lower = msg.to_ascii_lowercase();
            if lower.contains("error")
                || lower.contains("failed")
                || lower.contains("invalid")
                || lower.contains("forbidden")
                || lower.contains("unauthorized")
            {
                Some(msg)
            } else {
                None
            }
        }
        _ => None,
    }
}
