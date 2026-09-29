# SPEC.md — Add Intel NPU / OpenVINO Whisper transcription to Meetily Community Edition

## Status

Implementation-ready specification for a coding agent.

## Target repository

Upstream project: **Meetily Community Edition** (`Zackriya-Solutions/meetily`).

Start from a **fresh fork of the current upstream `main` branch**. Do not implement directly on `main`.

Create a feature branch:

```bash
git fetch upstream
git checkout main
git reset --hard upstream/main
git checkout -b enhance/openvino-npu-transcription
git rev-parse HEAD
```

Record the resulting base commit SHA in the implementation notes / PR description before making changes.

The repository may have moved since this specification was written. Treat all file paths below as the verified architecture baseline, but inspect the current files before editing and adapt narrowly if upstream changed.

---

# 1. Mission

Add a new **local transcription provider for Intel NPUs on Windows** to Meetily Community Edition.

The provider must use **OpenVINO GenAI Whisper** and execute explicitly on the Intel NPU (`device = "NPU"`).

The result should feel like a first-class Meetily transcription option alongside the existing local Whisper and Parakeet providers.

The feature is intended for Windows 11 laptops with Intel Core Ultra processors / Intel AI Boost NPUs. The reference development machine is:

- Windows 11 x64
- Intel Core Ultra 7 265U
- Intel NPU available in Windows Task Manager
- no requirement for administrator rights to run Meetily after installation

The primary user wants to exploit the NPU for transcription while keeping the rest of Meetily unchanged.

---

# 2. Primary user experience

In Meetily transcription settings, the user should be able to select a provider similar to:

**Intel NPU (OpenVINO Whisper)**

Then choose one of the initially supported models:

- Whisper Base INT8
- Whisper Small INT8

Default for this provider:

- **Whisper Small INT8**

When the provider is selected, Meetily should:

1. detect whether an Intel NPU is available;
2. clearly report NPU availability;
3. download the selected compatible OpenVINO model if it is missing;
4. start a persistent local OpenVINO helper when transcription is needed;
5. compile/load the model for **NPU only**;
6. reuse the loaded model for all subsequent audio chunks;
7. send Meetily's existing 16 kHz mono float audio chunks to the helper;
8. return text through Meetily's existing transcription pipeline;
9. shut down or reload the helper cleanly when appropriate.

The user must never need Python, Node.js, a separate OpenVINO installation, or an OpenVINO environment setup just to run the packaged application.

---

# 3. Non-negotiable requirements

## 3.1 Explicit NPU execution

OpenVINO must be initialized with the explicit target:

```text
NPU
```

Do **not** use:

```text
AUTO
```

Do not silently fall back to:

- CPU
- GPU
- DirectML
- Vulkan
- another OpenVINO device

If the NPU is unavailable or model compilation for the NPU fails, return a clear actionable error to the UI.

The purpose of this provider is specifically to use the NPU.

## 3.2 Existing providers must remain unchanged

Do not replace or rewrite existing transcription backends.

Existing behavior for at least these providers must remain intact:

- local Whisper / whisper.cpp
- Parakeet

Avoid unrelated refactors.

## 3.3 Windows-first implementation

The new OpenVINO NPU provider is a **Windows-only feature for v1**.

Requirements:

- existing macOS and Linux builds must continue compiling;
- Windows-specific Rust code should be appropriately guarded;
- the frontend should either hide this provider on unsupported platforms or show it disabled with a concise explanation;
- do not introduce OpenVINO runtime dependencies into non-Windows packages.

## 3.4 No administrator rights at runtime

The packaged application must not require users to:

- install OpenVINO globally;
- set OpenVINO environment variables manually;
- run an OpenVINO installer;
- install Python;
- run Meetily as Administrator.

Bundle/deploy the required OpenVINO runtime components with the application according to the current official OpenVINO redistribution/deployment guidance.

## 3.5 Privacy

All transcription remains local.

Do not:

- upload audio;
- upload transcripts;
- add telemetry containing transcript text;
- add any cloud fallback.

---

# 4. Verified Meetily architecture baseline

Before modifying anything, inspect the current equivalents of the following files.

The upstream architecture at specification time contains:

```text
frontend/src-tauri/src/audio/transcription/provider.rs
frontend/src-tauri/src/audio/transcription/whisper_provider.rs
frontend/src-tauri/src/audio/transcription/engine.rs
frontend/src-tauri/src/audio/transcription/worker.rs
frontend/src-tauri/src/audio/pipeline.rs
frontend/src-tauri/src/audio/retranscription.rs
frontend/src-tauri/src/api/api.rs

frontend/src/components/TranscriptSettings.tsx
frontend/src/constants/modelDefaults.ts

frontend/src-tauri/tauri.conf.json
frontend/src-tauri/Cargo.toml
.github/workflows/build-windows.yml

llama-helper/
Cargo.toml
CLAUDE.md
```

The current transcription abstraction is conceptually:

```rust
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> Result<TranscriptResult, TranscriptionError>;

    async fn is_model_loaded(&self) -> bool;
    async fn get_current_model(&self) -> Option<String>;
    fn provider_name(&self) -> &'static str;
}
```

The audio passed to providers is already intended to be:

- mono
- 16 kHz
- `f32`

`TranscriptionEngine` currently contains direct legacy/backward-compatible variants for Whisper and Parakeet plus a preferred generic provider path roughly equivalent to:

```rust
TranscriptionEngine::Provider(Arc<dyn TranscriptionProvider>)
```

**Use the generic provider path for the OpenVINO implementation.**

Do not add a third large special-case transcription implementation directly into `worker.rs`.

---

# 5. Required architecture

Use this architecture unless current upstream code has evolved to provide an objectively cleaner equivalent.

```text
Meetily Tauri/Rust
    |
    | existing audio capture / mixing / VAD / chunking
    | 16 kHz mono Vec<f32>
    v
OpenVinoWhisperProvider
    |
    | persistent framed IPC over child stdin/stdout
    v
openvino-whisper-helper.exe
    |
    | OpenVINO GenAI C++
    | ov::genai::WhisperPipeline
    | device = "NPU"
    v
Intel NPU
```

## Why a sidecar

Prefer a small native C++ sidecar instead of introducing Python at runtime.

The repository already has a sidecar packaging pattern through `llama-helper` and Tauri `externalBin`. Follow the same general deployment approach.

The sidecar must be persistent so the model is **not reloaded or recompiled for each Meetily audio chunk**.

---

# 6. New components

Create a logical structure similar to the following. Exact filenames can follow repository conventions.

```text
openvino-whisper-helper/
    CMakeLists.txt
    src/
        main.cpp
        protocol.*
        whisper_engine.*
        device_probe.*
    tests/
    README.md

frontend/src-tauri/src/audio/transcription/
    openvino_whisper_provider.rs
    openvino_helper_client.rs
    ... existing files ...

frontend/src/components/
    OpenVinoWhisperModelManager.tsx
    ... existing files ...
```

Do not force this exact split if a smaller structure is more idiomatic, but keep:

- OpenVINO C++ inference code outside the Rust backend;
- sidecar lifecycle/IPC code separated from provider business logic;
- frontend model management separated from existing whisper.cpp GGML model handling.

---

# 7. OpenVINO version strategy

Use a single pinned, internally consistent OpenVINO release family.

At specification time, the current stable OpenVINO line is **2026.4**. Prefer that version family unless the current upstream release has materially changed by implementation time.

All OpenVINO components used by the helper must be compatible versions:

- OpenVINO runtime
- OpenVINO GenAI
- OpenVINO tokenizers if required by the selected deployment
- NPU plugin/runtime components

Do not combine arbitrary runtime/model generations.

The earlier experiment that motivated this work encountered an OpenVINO IR load failure of the form:

```text
Cannot create Truncate layer ... from unsupported opset: extension
```

Therefore model/runtime compatibility is a hard requirement.

Do not download a floating model revision from `main` and assume it will remain compatible forever.

---

# 8. Whisper model strategy

Initially support only:

```text
whisper-base-int8
whisper-small-int8
```

Use the multilingual Whisper models, not `.en` variants.

The implementation must be verified with **German transcription**.

## Model source

Preferred approach:

1. use an official OpenVINO-converted Whisper artifact;
2. pin a specific known-good revision;
3. verify it against the bundled OpenVINO runtime;
4. store that revision and expected artifact metadata in a Meetily model manifest.

If the official preconverted artifact cannot be made stable/reproducible, add a developer/release-time export process from the upstream multilingual OpenAI Whisper model using the same OpenVINO version family.

End users must **not** need the export toolchain.

## Model manifest

Create a manifest/data structure containing at least:

```text
provider id
display name
model id
model source/revision
OpenVINO compatibility version
expected required files
download metadata
integrity hashes where feasible
```

Never rely on a floating remote branch.

## Storage

Keep OpenVINO models separate from whisper.cpp GGML models.

Conceptually:

```text
<Meetily app data>/
    models/
        openvino/
            whisper-base-int8/
                <pinned revision>/
                    ...
            whisper-small-int8/
                <pinned revision>/
                    ...
```

Use the application's existing path/config conventions rather than hardcoding a user-specific path.

Downloads should:

- use a temporary path first;
- support progress;
- detect interrupted/partial downloads;
- validate required files;
- perform an atomic final move/rename where possible;
- support retry and delete;
- never report a model as ready if required files are incomplete.

---

# 9. OpenVINO C++ helper

Implement a small native executable, working name:

```text
openvino-whisper-helper.exe
```

The helper must use the **OpenVINO GenAI C++ API**.

Core inference concept:

```cpp
ov::genai::WhisperPipeline pipeline(model_path, "NPU", properties);
auto result = pipeline.generate(audio, generation_config);
```

Use the exact API supported by the pinned OpenVINO version.

## 9.1 NPU probe

Before loading a model:

- initialize OpenVINO;
- enumerate available devices;
- require an Intel NPU device;
- return diagnostic information to Meetily.

If no NPU exists, return a typed error. Do not substitute another device.

Expose useful non-sensitive diagnostics such as:

```text
OpenVINO version
available OpenVINO devices
selected device
NPU device name if available
model id
model path/revision
cache directory
```

## 9.2 Model compile/load

Load once per selected model.

Use OpenVINO model caching for NPU compilation, following the current official Whisper C++ sample / current recommended API.

The cache directory must live under an appropriate Meetily application cache location, not the current working directory.

A second application run should benefit from the cache where OpenVINO supports it.

## 9.3 Audio format

Input from Meetily is expected as:

```text
16,000 Hz
mono
normalized f32
approximately [-1.0, +1.0]
```

Validate sample rate in the protocol.

Do not independently resample in the helper unless required for defensive validation. The existing Meetily pipeline should remain the source of truth for resampling.

## 9.4 Generation configuration

Default behavior:

- task: `transcribe`
- not `translate`
- language hint supplied when user configured one
- no word-level timestamps in v1
- no speaker diarization in this provider
- no LLM cleanup inside the helper
- no hallucination post-processing unique to OpenVINO unless Meetily already applies shared filtering downstream

`TranscriptResult.confidence` may be `None` if OpenVINO does not expose a confidence value directly comparable to existing providers.

Return:

```text
is_partial = false
```

for the initial provider implementation unless Meetily has an established partial-result contract that can be correctly implemented.

---

# 10. IPC protocol

The helper must be a persistent child process controlled by the Rust backend.

Use a robust length-framed protocol over stdin/stdout.

Do **not** JSON-encode thousands of float samples.

Suggested framing:

## Request frame

```text
uint32_le json_header_length
json_header bytes (UTF-8)
optional raw audio bytes (sample_count * 4, little-endian IEEE-754 f32)
```

Example header:

```json
{
  "protocol": 1,
  "id": 42,
  "op": "transcribe",
  "sample_rate": 16000,
  "sample_count": 123456,
  "language": "de"
}
```

## Response frame

```text
uint32_le json_length
json bytes (UTF-8)
```

Example:

```json
{
  "protocol": 1,
  "id": 42,
  "ok": true,
  "text": "Guten Morgen zusammen.",
  "model": "whisper-small-int8",
  "device": "NPU",
  "inference_ms": 812
}
```

Errors:

```json
{
  "protocol": 1,
  "id": 42,
  "ok": false,
  "error_code": "NPU_COMPILE_FAILED",
  "error": "..."
}
```

## Required operations

At minimum:

```text
hello
probe
load_model
transcribe
unload_model
shutdown
```

A separate `health` operation is recommended.

## Protocol hygiene

- stdout is protocol-only;
- logs go to stderr;
- never print banners/debug text to stdout;
- reject unsupported protocol versions cleanly;
- validate all frame lengths;
- impose sane maximum frame/audio sizes;
- handle EOF and child termination without deadlocks.

Because Meetily's current live transcription worker is effectively serialized, it is acceptable for v1 to serialize helper inference requests.

---

# 11. Sidecar lifecycle

Implement a Rust-side client that owns the child process and coordinates requests.

Expected lifecycle:

```text
provider selected
    |
recording / retranscription begins
    |
helper starts if needed
    |
probe NPU
    |
load selected model once
    |
many transcribe requests
    |
reuse model
    |
recording ends
    |
helper may remain available for reuse or shut down according to clean app lifecycle
```

Requirements:

- never spawn one helper per audio chunk;
- never compile the model once per audio chunk;
- handle model switch by unloading/restarting safely;
- kill/cleanup the child on application shutdown;
- avoid orphan helper processes;
- protect writes/reads with a single request queue or mutex;
- use unique request IDs;
- apply reasonable timeouts;
- surface stderr on failure without leaking transcript text.

## Crash policy

If the helper crashes unexpectedly:

1. mark the provider unhealthy;
2. collect the exit code / last diagnostic stderr;
3. optionally restart the helper **once**;
4. retry the current chunk at most once if it is safe to do so;
5. if the second attempt fails, stop using the provider and surface an actionable transcription error.

Do not fall back to CPU/GPU.

---

# 12. Rust provider integration

Create an implementation equivalent to:

```text
OpenVinoWhisperProvider
```

which implements Meetily's existing:

```text
TranscriptionProvider
```

The provider should:

- hold/share the helper client;
- know the selected model;
- expose model-loaded status;
- expose provider name;
- translate helper errors to Meetily `TranscriptionError`;
- map language settings;
- return `TranscriptResult`.

Recommended provider identifier:

```text
openvinoWhisper
```

Recommended display label:

```text
Intel NPU (OpenVINO Whisper)
```

Use a stable identifier everywhere.

---

# 13. Integrate through the generic provider path

In the current architecture, `TranscriptionEngine` includes a generic:

```text
Provider(Arc<dyn TranscriptionProvider>)
```

Use this for OpenVINO.

Update the engine initialization logic so conceptually:

```text
provider == "openvinoWhisper"
    -> initialize OpenVinoWhisperProvider
    -> TranscriptionEngine::Provider(...)
```

Do not add an `OpenVino(...)` enum branch unless the generic provider abstraction has disappeared upstream or there is a compelling type/lifecycle reason.

If you must deviate, document why.

---

# 14. Audit provider-specific assumptions

Search the entire repository for assumptions that only know about:

```text
localWhisper
whisper
parakeet
```

At minimum audit:

```bash
rg -n '"localWhisper"|localWhisper|"parakeet"|parakeet|"whisper"|whisper' frontend/src frontend/src-tauri
```

Pay special attention to:

- recording start/readiness checks;
- model-ready validation;
- transcription engine initialization;
- settings persistence;
- model download gating;
- onboarding;
- import/retranscription;
- frontend provider union types;
- defaults;
- test endpoints;
- provider-specific error messages.

Do not globally change unrelated uses of the words Whisper/Parakeet; inspect each match.

Historically this repository has had provider-readiness bugs caused by hardcoded provider assumptions. The new provider must not be blocked by checks intended for another provider.

---

# 15. Configuration and persistence

Current transcript configuration is generic enough to store strings such as provider/model.

Prefer reusing the existing configuration format:

```text
provider
model
api_key
```

For OpenVINO:

```text
provider = "openvinoWhisper"
model = "whisper-small-int8"
api_key = null
```

Do not create a database migration unless the existing schema genuinely requires one.

If a migration is required, make it backward compatible.

Add frontend default constants in the existing model defaults system and keep Rust/frontend defaults synchronized.

---

# 16. Frontend settings UI

Extend the existing transcription settings rather than creating a separate settings page.

Add:

```text
Intel NPU (OpenVINO Whisper)
```

to the local provider selection on Windows.

The OpenVINO section should show at least:

- provider description;
- NPU detected / not detected;
- selected NPU device if available;
- OpenVINO runtime version;
- model selection;
- model download state/progress;
- load/compile error;
- retry action;
- delete model action;
- explicit statement that this provider runs locally.

Suggested concise description:

> Runs multilingual Whisper locally on a supported Intel NPU using OpenVINO. No CPU/GPU fallback.

If NPU is unavailable:

> Intel NPU not available. Update the Intel NPU driver or choose another transcription provider.

Do not imply that all Windows PCs contain a supported Intel NPU.

---

# 17. Model manager UI

Implement a dedicated OpenVINO Whisper model manager or a clean reuse of existing model-manager primitives.

Do not mix OpenVINO model directories with whisper.cpp `.bin` / GGML models.

Supported initial UI choices:

```text
Whisper Base INT8
Whisper Small INT8
```

Default:

```text
Whisper Small INT8
```

The model manager must be able to distinguish:

```text
not downloaded
downloading
downloaded
validating
ready
failed
```

If practical, also distinguish:

```text
NPU compiling/loading
loaded
```

---

# 18. Language mapping

Meetily already passes an optional language value into the provider abstraction.

Implement a dedicated mapping layer for OpenVINO Whisper.

Requirements:

- `None` / automatic detection -> do not force a language token;
- `"de"` -> German;
- `"en"` -> English;
- other supported Meetily Whisper languages -> map correctly;
- unsupported explicit language -> return a clear `UnsupportedLanguage` error rather than silently switching language.

OpenVINO/Whisper APIs may represent languages using special Whisper tokens such as:

```text
<|de|>
<|en|>
```

Use the exact generation config required by the pinned OpenVINO API.

If Meetily has an existing `auto-translate` or translation mode:

- preserve current semantics;
- map it to Whisper task `translate` only when the user explicitly selected translation;
- ordinary German transcription must remain German and use task `transcribe`.

Add tests for German.

---

# 19. Recording/live transcription

Live Meetily recording must support the new provider.

Do not replace:

- microphone capture;
- system-audio capture;
- mixer;
- VAD;
- existing chunk construction;
- timestamps;
- transcript persistence.

Reuse the existing pipeline.

The OpenVINO provider receives the speech chunks after existing Meetily processing.

The user should be able to:

1. choose OpenVINO Whisper;
2. choose/download Small INT8;
3. start a meeting recording;
4. see live transcript chunks appear;
5. stop the meeting;
6. retain the transcript exactly through Meetily's existing persistence path.

No additional manual command-line process should be required.

---

# 20. File import / retranscription

This feature is not complete if the provider only works for live recordings.

Meetily's current file import/retranscription code has historically used separate direct Whisper/Parakeet engine paths rather than only the generic live provider abstraction.

Audit:

```text
frontend/src-tauri/src/audio/retranscription.rs
```

and related import code.

Add OpenVINO provider support to file retranscription/import while preserving existing behavior.

Preferred approach:

- reuse `OpenVinoWhisperProvider` rather than duplicating OpenVINO inference code;
- reuse decoded/resampled audio chunks;
- keep the helper persistent across the complete imported file;
- do not compile/load the model once per chunk.

User acceptance case:

> Import a previously recorded ~32-minute meeting, choose OpenVINO Whisper Small INT8, transcribe it completely on the NPU, and save the transcript in Meetily.

---

# 21. OpenVINO cache

Configure an application-owned OpenVINO compile cache for NPU use.

Requirements:

- use an appropriate Meetily cache directory;
- do not write cache files into the repository or application installation directory;
- keep cache versionable/clearable;
- survive normal app restarts;
- avoid stale-cache crashes when OpenVINO/model versions change.

If necessary, include the OpenVINO/runtime version and model revision in the cache namespace.

---

# 22. Diagnostics

Add enough diagnostics to debug this feature without adding excessive logging.

At minimum make available in logs/status:

```text
provider = openvinoWhisper
OpenVINO runtime version
selected model id
pinned model revision
available devices
selected device = NPU
helper version/protocol version
model load/compile duration
cache directory / cache-hit indication if available
audio duration per request
inference duration per request
RTF per request or aggregate
```

RTF definition:

```text
RTF = inference_time / audio_duration
```

Do not log full audio or add new logging of transcript contents solely for this provider.

A diagnostic UI command/button such as `Test NPU` is desirable but not required if model loading itself gives clear feedback.

---

# 23. Error handling

Create actionable error categories.

Examples:

```text
NPU_NOT_AVAILABLE
NPU_DRIVER_ERROR
OPENVINO_RUNTIME_LOAD_FAILED
MODEL_NOT_FOUND
MODEL_INVALID
MODEL_RUNTIME_INCOMPATIBLE
NPU_COMPILE_FAILED
HELPER_START_FAILED
HELPER_CRASHED
IPC_PROTOCOL_ERROR
TRANSCRIPTION_FAILED
UNSUPPORTED_LANGUAGE
```

Map these to clear user-facing messages.

Particularly, if OpenVINO reports an IR/opset/model compatibility problem, the UI should not simply say "Transcription failed".

A useful message is conceptually:

> The selected OpenVINO Whisper model is incompatible with the bundled OpenVINO runtime. Re-download the model or update Meetily.

Do not show raw stack traces as the only user-facing message. Preserve technical details in logs.

---

# 24. NPU driver handling

Do not install or update device drivers from Meetily.

When the NPU is missing or OpenVINO reports a driver incompatibility:

- report that the Intel NPU is unavailable;
- show the detected OpenVINO devices;
- advise the user to update the Intel NPU driver through their OEM/Intel-supported channel;
- leave existing providers usable.

No elevated privilege request should be triggered.

---

# 25. Packaging

Use the existing Tauri sidecar packaging pattern.

Update:

```text
frontend/src-tauri/tauri.conf.json
```

so the helper is included as an external binary similarly to the existing helper binaries.

Expected conceptual external binary entry:

```text
binaries/openvino-whisper-helper
```

Build output naming must match Tauri's target-triple sidecar convention, for example on Windows x64:

```text
openvino-whisper-helper-x86_64-pc-windows-msvc.exe
```

Follow the repository's existing approach rather than inventing a second packaging mechanism.

Bundle all runtime DLLs/plugins required for OpenVINO NPU inference in an application-local location supported by the helper.

The installed app must work on a compatible Windows 11 machine without a separately installed OpenVINO SDK.

---

# 26. Build system

Add the C++ helper to the repository's normal Windows development/release workflow.

Prefer:

- CMake for the helper;
- MSVC toolchain compatible with the existing Windows build prerequisites;
- deterministic/pinned OpenVINO dependency acquisition.

Do not download arbitrary latest build dependencies at runtime.

Developer setup may require the already-documented Visual Studio C++ build tools.

Runtime must not require build tools.

---

# 27. CI

Extend the Windows workflow to:

1. acquire the pinned OpenVINO development/runtime package;
2. configure/build `openvino-whisper-helper`;
3. run helper unit tests that do not require an NPU;
4. copy the sidecar binary into the Tauri binaries directory with the correct target-triple name;
5. include required runtime libraries;
6. build the normal Meetily Windows artifact.

CI runners are not expected to have an Intel NPU.

Therefore:

- compilation and protocol tests must pass without NPU hardware;
- NPU hardware integration is a separate manual test;
- absence of an NPU in CI must not be treated as a build failure.

Do not weaken existing Windows build/test coverage.

---

# 28. Tests

Add automated coverage appropriate to the repository.

## 28.1 Rust unit tests

At minimum:

- provider config/model mapping;
- language mapping (`de`, `en`, auto);
- helper protocol request encoding;
- helper response decoding;
- oversized/malformed frame rejection;
- error-code mapping;
- no-fallback behavior;
- helper lifecycle state transitions where testable.

## 28.2 C++ helper tests

At minimum:

- frame parser;
- JSON/header validation;
- invalid protocol version;
- sample count / byte count mismatch;
- unsupported sample rate;
- probe response serialization;
- model-load error serialization.

Abstract the actual OpenVINO pipeline enough to test protocol/control flow without physical NPU hardware.

## 28.3 Frontend tests

Cover:

- provider option visibility on Windows;
- model selection;
- model readiness states;
- NPU-unavailable state;
- config save/load with `openvinoWhisper`.

Follow the current project's testing style instead of introducing a new framework unnecessarily.

## 28.4 Regression tests

Existing providers must still work logically:

```text
localWhisper
parakeet
```

Ensure the new provider does not become the default globally.

---

# 29. Manual hardware acceptance test

Run this on the reference machine if available:

```text
Windows 11
Intel Core Ultra 7 265U
```

## Test A — device verification

1. Select `Intel NPU (OpenVINO Whisper)`.
2. Select/download `Whisper Small INT8`.
3. Start model/inference.
4. Open Windows Task Manager -> Performance -> NPU.
5. Confirm NPU utilization rises during inference.
6. Confirm no silent fallback is reported.
7. Confirm logs say device `NPU`.

Pass condition:

- OpenVINO provider uses NPU;
- CPU/GPU may have normal orchestration load, but inference must not silently execute through another OpenVINO target.

## Test B — German transcription

Use a German meeting/monologue.

Verify:

- German remains German;
- reasonable punctuation;
- English technical terms embedded in German do not force translation;
- no forced English output;
- output reaches Meetily transcript storage.

## Test C — persistent model

Transcribe multiple consecutive chunks / a meeting.

Verify:

- helper starts once;
- model loads/compiles once;
- the model is reused;
- there is no per-chunk compilation delay.

## Test D — import/retranscription

Use the existing reference recording of approximately 32 minutes.

Record:

```text
audio duration
model
initial helper startup time
model compile/load time
pure inference time
total transcription time
aggregate RTF
average NPU utilization if practical
```

Reference only, not a hard pass/fail target:

A separate Vibe + Parakeet TDT 0.6B v3 test on the same machine transcribed roughly 32 minutes in approximately 101 seconds (~19x realtime). The OpenVINO Whisper provider does **not** need to beat Parakeet to be valid; its goal is efficient, explicit NPU execution.

Desired practical target:

```text
RTF < 1.0 after warm-up
```

Treat failure to achieve this as a performance investigation item, not permission to fall back to GPU/CPU.

---

# 30. Performance rules

The following are defects:

- starting a new helper for every chunk;
- reloading the model for every chunk;
- recompiling the NPU model for every chunk;
- serializing audio as huge JSON arrays;
- copying audio repeatedly without need;
- executing through OpenVINO `AUTO`;
- retry loops that repeatedly recompile the model.

Optimize correctness/lifecycle first.

Do not micro-optimize the existing audio pipeline unless profiling proves it necessary.

---

# 31. Security / robustness

The helper is a local trusted child process, but still validate IPC inputs.

Requirements:

- bounded frame sizes;
- bounded model paths;
- no shell interpolation for model paths;
- spawn helper using argument arrays / native process APIs;
- no arbitrary command execution;
- stdout protocol parser fails closed on malformed data;
- models are stored only in application-controlled directories;
- verify downloaded artifacts where practical;
- no remote code/model execution flags at runtime.

If model download metadata can be compromised, checksums should detect corruption/tampering.

---

# 32. Licensing

Meetily is open-source; the implementation must also respect licenses of:

- OpenVINO runtime/GenAI components;
- any redistributed runtime libraries;
- Whisper model artifacts;
- model hosting source.

Add or update third-party notices as required.

Do not copy code from incompatible-licensed example projects.

Using official OpenVINO sample APIs as reference is fine; keep implementation original and appropriately attributed where required.

---

# 33. Documentation

Add a concise user-facing section covering:

## Intel NPU transcription

Include:

- Windows-only initial support;
- supported Intel NPU requirement;
- local/offline processing statement;
- Base INT8 / Small INT8;
- how to select provider;
- how to download model;
- how to diagnose "NPU unavailable";
- no CPU/GPU fallback;
- model load/compile may be slower on first use due to NPU compilation/cache;
- subsequent starts may be faster due to cache.

Add developer documentation for:

- helper build;
- pinned OpenVINO version;
- model manifest/revision;
- sidecar protocol;
- runtime packaging;
- manual NPU validation.

---

# 34. Likely files to modify

This list is guidance, not a license to edit blindly.

Backend:

```text
frontend/src-tauri/src/audio/transcription/provider.rs
frontend/src-tauri/src/audio/transcription/engine.rs
frontend/src-tauri/src/audio/transcription/worker.rs
frontend/src-tauri/src/audio/retranscription.rs
frontend/src-tauri/src/api/api.rs
frontend/src-tauri/src/lib.rs or module registration files
frontend/src-tauri/Cargo.toml
```

Prefer **minimal or no changes to `worker.rs`** beyond what is required by generic provider initialization.

New backend files likely:

```text
frontend/src-tauri/src/audio/transcription/openvino_whisper_provider.rs
frontend/src-tauri/src/audio/transcription/openvino_helper_client.rs
```

Frontend:

```text
frontend/src/components/TranscriptSettings.tsx
frontend/src/constants/modelDefaults.ts
```

Potential new frontend component:

```text
frontend/src/components/OpenVinoWhisperModelManager.tsx
```

Packaging/build:

```text
frontend/src-tauri/tauri.conf.json
.github/workflows/build-windows.yml
Cargo.toml
```

New helper:

```text
openvino-whisper-helper/
```

Also inspect all provider comparisons and current tests.

---

# 35. Implementation order

Follow this order unless repository changes make a different sequence clearly safer.

## Phase 0 — repository reconnaissance

Before editing:

1. read repository `CLAUDE.md` / agent instructions;
2. record base commit SHA;
3. inspect existing provider trait;
4. inspect WhisperProvider;
5. inspect engine initialization;
6. inspect live worker;
7. inspect retranscription/import;
8. inspect transcript settings/model managers;
9. inspect `llama-helper` build/packaging;
10. inspect Windows CI;
11. run baseline tests/builds relevant to the change.

Write a short implementation note summarizing any differences from this spec.

Do not ask the user to resolve ordinary implementation choices; make the smallest architecture-consistent choice.

## Phase 1 — helper skeleton

Implement:

- CMake project;
- version/protocol;
- framed IPC;
- `hello`;
- `probe`;
- stderr logging;
- testable OpenVINO abstraction;
- Windows build.

No Meetily integration required yet.

## Phase 2 — OpenVINO Whisper engine

Implement:

- NPU device detection;
- explicit `NPU` target;
- pinned OpenVINO dependency;
- model load;
- compile cache;
- multilingual Whisper generation;
- German language;
- `transcribe`;
- clean errors;
- no fallback.

Validate with a small local audio sample on NPU hardware when available.

## Phase 3 — Rust helper client/provider

Implement:

- child process lifecycle;
- framed binary protocol;
- provider trait;
- request serialization;
- response/error mapping;
- model state;
- clean shutdown;
- bounded restart behavior.

## Phase 4 — config/UI/model management

Implement:

- `openvinoWhisper` provider;
- Windows provider option;
- defaults;
- model manager;
- download/validation;
- NPU status;
- settings persistence.

## Phase 5 — live recording

Wire engine creation through `TranscriptionEngine::Provider`.

Audit provider readiness gates.

Verify a real live meeting can record/transcribe.

## Phase 6 — import/retranscription

Integrate the same provider/helper with existing import/retranscription.

Ensure the model stays loaded across the whole file.

## Phase 7 — packaging/CI

Bundle:

- helper;
- OpenVINO runtime dependencies;
- plugin dependencies;
- version/notice files as required.

Update Windows CI.

## Phase 8 — tests/docs/manual benchmark

Finish all automated tests, docs, and manual hardware test notes.

---

# 36. Build and validation commands

Use the repository's exact current commands if they changed.

At specification time, expected frontend commands include:

```bash
cd frontend
pnpm install --frozen-lockfile
pnpm lint
pnpm build
pnpm tauri:dev
pnpm tauri:build
```

Rust workspace validation should include appropriate equivalents of:

```bash
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
```

Build/test the C++ sidecar separately with CMake/CTest as appropriate.

On Windows, validate the final packaged artifact, not only `cargo run`.

Do not declare completion while the sidecar works only from a developer shell with OpenVINO environment variables set.

---

# 37. Acceptance criteria

The implementation is complete only when all applicable items below are true.

## Functional

- [ ] `Intel NPU (OpenVINO Whisper)` appears as a Windows local transcription provider.
- [ ] Existing Whisper and Parakeet providers continue to work.
- [ ] Base INT8 can be downloaded/selected.
- [ ] Small INT8 can be downloaded/selected.
- [ ] Small INT8 is the OpenVINO provider default.
- [ ] German transcription works.
- [ ] automatic language mode works.
- [ ] settings persist across restart.
- [ ] live recording transcribes through the NPU provider.
- [ ] imported media/retranscription works through the same provider.
- [ ] transcript results are persisted through Meetily's existing flow.

## NPU correctness

- [ ] OpenVINO is targeted with `NPU`, not `AUTO`.
- [ ] NPU availability is checked before inference.
- [ ] no silent CPU fallback exists.
- [ ] no silent GPU fallback exists.
- [ ] selected execution device is diagnosable.
- [ ] Intel NPU activity is observable during manual hardware test.

## Lifecycle/performance

- [ ] helper remains persistent across chunks.
- [ ] model is not reloaded per chunk.
- [ ] model is not recompiled per chunk.
- [ ] OpenVINO cache is application-owned.
- [ ] second use can reuse cache when supported.
- [ ] aggregate RTF is reported during benchmark.
- [ ] target RTF after warm-up is < 1.0 on reference hardware if technically achievable.

## Packaging

- [ ] no Python runtime required.
- [ ] no global OpenVINO installation required.
- [ ] no Administrator rights required to run the packaged app.
- [ ] Windows package contains helper and required OpenVINO runtime components.
- [ ] app works from a clean normal user account on compatible hardware.
- [ ] non-Windows builds remain unaffected.

## Reliability

- [ ] missing NPU produces a clear error.
- [ ] incompatible model/runtime produces a clear error.
- [ ] partial/corrupt model downloads are detected.
- [ ] helper crash is handled without hanging Meetily.
- [ ] helper processes do not remain orphaned after app exit.
- [ ] malformed IPC does not crash the parent app.

## Quality

- [ ] automated tests added.
- [ ] existing relevant tests pass.
- [ ] lint/build pass.
- [ ] docs updated.
- [ ] third-party notices handled.
- [ ] no unrelated large refactor.

---

# 38. Explicit non-goals for v1

Do not expand scope to:

- macOS OpenVINO support;
- Linux OpenVINO support;
- AMD NPU;
- Qualcomm NPU;
- DirectML;
- CUDA;
- replacing whisper.cpp;
- replacing Parakeet;
- speaker diarization inside OpenVINO;
- word-level timestamp UI;
- streaming token-by-token Whisper decoding;
- LLM summarization changes;
- Obsidian integration;
- cloud transcription;
- generic arbitrary OpenVINO model execution;
- Medium/Large/Turbo Whisper NPU models unless needed to prove architecture;
- automatic driver installation.

These can be future work.

---

# 39. Important implementation constraints

1. **Do not solve this by spawning a Python process.**
2. **Do not use OpenVINO AUTO for the NPU provider.**
3. **Do not silently fall back.**
4. **Do not serialize PCM floats as JSON.**
5. **Do not reload/recompile per chunk.**
6. **Do not modify existing Whisper/Parakeet engines more than necessary.**
7. **Do not reuse whisper.cpp GGML model files for OpenVINO.**
8. **Do not download unpinned model revisions.**
9. **Do not require a global OpenVINO install.**
10. **Do not declare success based only on CPU-only CI.**
11. **Do not break file import/retranscription.**
12. **Do not hardcode user-specific paths.**

---

# 40. Agent autonomy

You are expected to implement this feature end-to-end.

Do not stop merely because a file path or function name changed from this specification. Search the repository, find the current equivalent, and proceed.

If a requirement conflicts with current upstream architecture:

1. preserve the intent of the requirement;
2. choose the smallest maintainable change;
3. document the deviation in the final implementation report.

Only leave a requirement unimplemented if there is a concrete technical blocker that cannot reasonably be solved in the repository. If that occurs, document:

- exact blocker;
- evidence;
- affected acceptance criterion;
- best next step.

Do not substitute a CPU/GPU implementation for an NPU blocker.

---

# 41. Final deliverables from the coding agent

When implementation is complete, provide:

1. **Summary of architecture**
2. **Base upstream commit SHA**
3. **List of changed/new files**
4. **Pinned OpenVINO version**
5. **Pinned model revisions**
6. **How OpenVINO runtime is packaged**
7. **How no-fallback-to-CPU/GPU is enforced**
8. **Build commands**
9. **Test commands and results**
10. **Manual NPU test results, if NPU hardware was available**
11. **Benchmark results**
12. **Known limitations**
13. **Any deviations from this SPEC**
14. **Ready-to-use PR description**

Benchmark table format:

```text
Hardware:
OS:
Intel NPU driver:
OpenVINO:
Model:

Audio duration:
Initial model load/compile:
Warm model inference time:
Total time:
RTF:
Observed NPU utilization:
Observed CPU utilization:
Observed GPU utilization:
German quality notes:
```

---

# 42. Definition of done

The core proof is:

> On a compatible Windows 11 Intel Core Ultra system, Meetily can select `Intel NPU (OpenVINO Whisper)`, download a pinned multilingual Whisper Small INT8 model, transcribe live or imported German audio through a persistent OpenVINO GenAI sidecar explicitly targeting `NPU`, visibly exercise the Intel NPU, persist the transcript through Meetily's normal workflow, and run without a separately installed OpenVINO/Python environment or administrator privileges.

Existing Whisper and Parakeet functionality must remain intact.

Start with repository reconnaissance, record the base SHA, then implement the feature through completion.
