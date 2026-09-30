// audio/transcription/mod.rs
//
// Transcription module: Provider abstraction, engine management, and worker pool.

pub mod provider;
pub mod whisper_provider;
pub mod parakeet_provider;
pub mod engine;
pub mod worker;
#[cfg(target_os = "windows")]
pub mod openvino_helper_client;
#[cfg(target_os = "windows")]
pub mod openvino_models;
#[cfg(target_os = "windows")]
pub mod openvino_whisper_provider;

// Re-export commonly used types
pub use provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
pub use whisper_provider::WhisperProvider;
pub use parakeet_provider::ParakeetProvider;
#[cfg(target_os = "windows")]
pub use openvino_whisper_provider::OpenVinoWhisperProvider;
pub use engine::{
    TranscriptionEngine,
    validate_transcription_model_ready,
    get_or_init_transcription_engine,
    get_or_init_whisper
};
pub use worker::{
    start_transcription_task,
    reset_speech_detected_flag,
    TranscriptUpdate
};
