# OpenVINO NPU implementation report

## Baseline and scope

- Upstream base: `a2cb62e827da7ef59f65064c97233efb2313878e` (`upstream/main`). The existing feature branch is `feature/openvino-npu`; its starting commit `ffd3de27651cd802939228525783b3509c56d70c` adds `SPEC.md` on that upstream base.
- Windows only: `Intel NPU (OpenVINO Whisper)` with multilingual Base INT8 and Small INT8; Small is the default.
- OpenVINO GenAI archive: `2026.4.0.0`, Runtime `2026.4.0`, archive SHA-256 `478a05c5ea30da26db3ae637477d553303cf38f070641fb4f218daf6d4c0a2e9`.
- Model revisions: Base `0fc9ee0ddbdde7eb70d7fda96ec76fb351f01b29`; Small `bbfa330248b664208618f19e08dfed5adf92b27a`.

## Architecture

Meetily's existing 16 kHz mono `f32` chunks enter `TranscriptionEngine::Provider`. `OpenVinoWhisperProvider` holds a persistent Rust client and one native C++ helper. Versioned, length-framed JSON headers carry metadata and binary little-endian `f32` payloads carry audio. The helper owns one `ov::genai::WhisperPipeline` and compiles it with the literal device `NPU`. It reuses the pipeline across recording chunks and batch segments. There is no OpenVINO `AUTO` target or CPU/GPU retry. A missing device or failed NPU compilation returns an error.

Model files live in application data under `models/openvino/<model>/<revision>`. The manifest pins every required file by size and SHA-256. Downloads use a staging directory, retries, verification, and an atomic directory swap. Per-model locks serialize model operations, and deletion refuses to remove a model used by a provider. The compilation cache is scoped by Runtime version, model, and revision.

Windows CI fetches and hashes the official GenAI SDK archive, builds and tests the helper with MSVC/CMake, and bundles the helper as a Tauri sidecar. It copies Core, GenAI, Tokenizers, IR frontend, the NPU plugin/compiler/VM runtime, TBB, and license notices into `openvino-runtime`. The helper launch prepends this directory to `PATH`. CPU, GPU, and AUTO plugins are excluded. CI additionally extracts the MSI and sends a `probe` request to the packaged helper using the packaged DLLs. Installed users need neither Python nor a separate OpenVINO installation.

## Files

- Native helper and protocol tests: `openvino-whisper-helper/`.
- Rust client, provider, model manager, and pinned manifest: `frontend/src-tauri/src/audio/transcription/openvino_*`.
- Recording, import, retranscription, commands, and cleanup: `frontend/src-tauri/src/audio/transcription/{engine.rs,mod.rs}`, `frontend/src-tauri/src/audio/{common.rs,import.rs,retranscription.rs}`, `frontend/src-tauri/src/lib.rs`, `frontend/src-tauri/Cargo.toml`.
- Frontend settings, readiness, model list, and tests: `frontend/src/components/{TranscriptSettings.tsx,LanguageSelection.tsx,OpenVinoWhisperModelManager.tsx}`, `frontend/src/constants/modelDefaults.ts`, `frontend/src/hooks/{useRecordingStart.ts,useTranscriptionModels.ts}`, `frontend/src/lib/transcription-model-readiness.ts`, `frontend/tests/lib/transcription-model-readiness.test.ts`.
- Packaging and user guidance: `.github/workflows/build-windows.yml`, `frontend/src-tauri/tauri.windows.conf.json`, `README.md`, `docs/INTEL_NPU_TRANSCRIPTION.md`, this report.

## Build and test

The Windows CI workflow contains the pinned SDK download, hash check, CMake build, `ctest`, Tauri build, OpenVINO Rust tests, MSI extraction, and packaged-helper probe. Local helper build commands are in [INTEL_NPU_TRANSCRIPTION.md](INTEL_NPU_TRANSCRIPTION.md#build-and-package-on-windows). The hardware-free Rust command is `cargo test --manifest-path frontend/src-tauri/Cargo.toml --target x86_64-pc-windows-msvc --features vulkan --lib audio::transcription::openvino_`; the frontend type check is `cd frontend; .\\node_modules\\.bin\\tsc.CMD --noEmit`. Run the full Rust suite on a machine with an audio output device.

Results in this workspace:

| Check | Result |
| --- | --- |
| TypeScript `tsc --noEmit` | Passed |
| Node readiness smoke checks | Passed: OpenVINO command mapping, unsupported provider, download status |
| `git diff --check` | Passed |
| Manifest shape and hash format | Passed: 19 required files and 19 SHA-256 hashes per model |
| SDK archive inspection | Passed: pin, C++ header signatures, required DLL names and layout inspected |
| Rust/C++ compilation and unit tests | Not run: `cargo`, `rustfmt`, CMake, and MSVC are unavailable locally |
| Next.js production build | Blocked by the existing `next/font` Google Fonts fetch in this network restricted environment |
| Next.js lint | No ESLint configuration exists; `next lint` opens its setup prompt |
| Bun frontend test file | Not run: Bun is unavailable locally; equivalent readiness smoke checks passed with Node |
| MSI package smoke test | Defined as a CI gate; no locally built MSI is available |

## Hardware acceptance and benchmark

This workspace does not provide a usable NPU test path: device inventory through `Get-CimInstance` was denied, and the native build toolchain is absent. NPU execution, German transcription quality, NPU utilization, warm cache behavior, and clean-account installer operation remain to be measured on the reference Windows 11 Core Ultra 7 265U machine. Do not treat CPU-only CI success as NPU acceptance.

```text
Hardware: Intel Core Ultra 7 265U (pending on reference machine)
OS: Windows 11 (pending build/version)
Intel NPU driver: pending
OpenVINO: 2026.4.0
Model: Whisper Small INT8, bbfa330248b664208618f19e08dfed5adf92b27a
Audio duration: approximately 32 minutes (reference audio; exact duration pending)
Initial model load/compile: pending
Warm model inference time: pending
Total time: pending
RTF: pending
Observed NPU utilization: pending
Observed CPU utilization: pending
Observed GPU utilization: pending
German quality notes: pending
```

## Deviations and limitations

- The pre-existing feature branch was retained rather than resetting and renaming the repository branch. Its parent is the verified upstream base above.
- The OpenVINO provider uses the existing generic `TranscriptionProvider` trait and a C++ sidecar so other engines and non-Windows packages remain isolated.
- Model-manager validation currently rehashes installed files on readiness checks, which can add latency for Small INT8. It prioritizes corruption detection.
- Windows CI and Intel NPU hardware acceptance have not been executed in this environment. Release requires both.

## Ready-to-use PR description

### Summary

Add Windows Intel NPU transcription using OpenVINO GenAI Whisper Base/Small INT8. The new provider uses a persistent native helper and explicit `NPU` targeting for recording, import, and retranscription. Existing Whisper and Parakeet providers remain available.

### Implementation

- Pin OpenVINO GenAI 2026.4.0.0 and immutable OpenVINO Whisper model revisions with per-file SHA-256 validation.
- Add binary framed helper IPC, provider lifecycle management, NPU probing, model downloads, settings UI, and diagnostics.
- Bundle the NPU-only OpenVINO runtime in the Windows installer and inspect/probe the packaged MSI in CI.

### Verification

- Passed: TypeScript type check, manifest inspection, `git diff --check`, SDK archive/header/layout inspection.
- Pending CI: Windows Rust/C++ build and tests, MSI extraction and packaged helper probe.
- Pending hardware: Windows 11 Core Ultra NPU live/import German transcription, utilization, clean install, and 32-minute benchmark.

### Rollout note

Do not merge as hardware-validated until the manual Intel NPU acceptance and benchmark in `docs/INTEL_NPU_TRANSCRIPTION.md` are completed.
