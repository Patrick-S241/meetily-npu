# Intel NPU transcription (Windows)

Meetily can run multilingual Whisper locally on a supported Intel NPU. The provider is **Intel NPU (OpenVINO Whisper)** and offers **Whisper Base INT8** and **Whisper Small INT8**. Small is the provider default. Audio and transcripts stay on the computer. OpenVINO is invoked with the explicit device `NPU`; this provider never switches to CPU or GPU.

## Use

1. On Windows 11 with an Intel Core Ultra NPU, open transcription settings and choose **Intel NPU (OpenVINO Whisper)**.
2. Check the NPU status and OpenVINO runtime version shown in the model manager. If the NPU is unavailable, update its driver using the laptop maker's or Intel's supported channel. Other Meetily providers remain available.
3. Download Base INT8 or Small INT8. Downloads are stored under Meetily application data in `models/openvino/<model>/<pinned-revision>/`, separate from whisper.cpp models. Interrupted, incomplete, or corrupted files do not count as ready; Retry and Delete are available.
4. Select a ready model and start a recording, import audio, or retranscribe a meeting. The first load may take longer while OpenVINO compiles for the NPU. Meetily stores the compilation cache in its application data, scoped by OpenVINO version and model revision, so later loads can be faster.

Meetily's normal installer bundles the native helper and its OpenVINO runtime. Running the installed application does not require Python, Node.js, a separate OpenVINO SDK, environment setup, or Administrator rights. An Intel NPU and a compatible driver are required. The app does not install device drivers.

## Architecture and versions

Meetily's existing 16 kHz mono `f32` audio chunks flow through `TranscriptionEngine::Provider` into `OpenVinoWhisperProvider`. A persistent Rust child client sends length-framed requests to `openvino-whisper-helper.exe`. The helper owns one `ov::genai::WhisperPipeline(model_path, "NPU", properties)` and reuses it across chunks. It runs `transcribe` by default; Meetily's explicit `auto-translate` option requests Whisper's `translate` task. Explicit `de` selects German transcription and automatic language mode does not force a language token.

Pinned distribution: [OpenVINO GenAI 2026.4.0.0 Windows x64 archive](https://storage.openvinotoolkit.org/repositories/openvino_genai/packages/2026.4/windows/openvino_genai_windows_2026.4.0.0_x86_64.zip), containing OpenVINO Runtime 2026.4.0 and matching GenAI/Tokenizers. Archive SHA-256: `478a05c5ea30da26db3ae637477d553303cf38f070641fb4f218daf6d4c0a2e9`. The [official installation guide](https://docs.openvino.ai/2026/get-started/install-openvino/install-openvino-genai.html) documents this complete archive and the [local distribution guide](https://docs.openvino.ai/2026/openvino-workflow/deployment-locally/local-distribution-libraries.html) describes DLL redistribution.

Model sources and immutable revisions:

| Meetily model | Official source | Revision |
| --- | --- | --- |
| Whisper Base INT8 | [OpenVINO/whisper-base-int8-ov](https://huggingface.co/OpenVINO/whisper-base-int8-ov) | `0fc9ee0ddbdde7eb70d7fda96ec76fb351f01b29` |
| Whisper Small INT8 | [OpenVINO/whisper-small-int8-ov](https://huggingface.co/OpenVINO/whisper-small-int8-ov) | `bbfa330248b664208618f19e08dfed5adf92b27a` |

`openvino_models_manifest.json` lists every downloaded file with size and SHA-256. The download manager fetches only URLs at the pinned revisions, stages files, validates them, and renames the staged directory into place. This manifest should be updated and tested before changing either the OpenVINO release or a model revision. The model cards say their IR requires OpenVINO 2025.2 or later, but a real NPU load test of this exact version pair remains necessary.

## Build and package on Windows

The simplest route to an installable test build is the repository's **Build and Test - Windows** GitHub Actions workflow. Commit and push this feature branch to your fork, then open **Actions → Build and Test - Windows → Run workflow** on that branch. Choose `release`, `sign-build: false`, and `upload-artifacts: true`. The unsigned path disables updater artifacts and skips code signing. After a successful run, download the workflow artifact and install either the `.msi` or the NSIS setup `.exe` inside it. The workflow exists only on a pushed branch; the current working-tree changes must be committed first.

For local development, install Node.js, pnpm 9.15.9, the stable Rust MSVC toolchain, Visual Studio C++ build tools, and CMake. From the repository root, run `pnpm --dir frontend install --frozen-lockfile`, build `llama-helper` with `cargo build --release -p llama-helper`, and place its binary at `frontend/src-tauri/binaries/llama-helper-x86_64-pc-windows-msvc.exe`. Build and stage the OpenVINO helper and DLLs as below. Then run `pnpm --dir frontend run tauri:dev:cpu` to start the desktop app. `pnpm --dir frontend dev` starts only the web frontend and cannot exercise the native NPU provider. The older `frontend/build-gpu.ps1` does not stage the OpenVINO helper and runtime.

The normal Windows CI job downloads the pinned GenAI archive, verifies its SHA-256, configures the helper with CMake/MSVC, runs the hardware-free protocol tests, and copies `openvino-whisper-helper-x86_64-pc-windows-msvc.exe` into `frontend/src-tauri/binaries/`. `tauri.windows.conf.json` adds this Windows-only sidecar. The CI also copies OpenVINO Core, GenAI, Tokenizers, IR frontend, NPU plugin/compiler/VM runtime, TBB, and licenses into `binaries/openvino-runtime/`; Tauri bundles this as `openvino-runtime/`. The Rust child prepends this folder to the helper's `PATH` at spawn. CI extracts the generated MSI and probes the packaged helper with its bundled DLLs. No OpenVINO CPU/GPU/AUTO plugins are copied into this runtime directory.

For a local development build, use the same archive and hash, extract it to a developer-selected directory, then run from the repository root:

```powershell
$sdk = 'C:\deps\openvino_genai_windows_2026.4.0.0_x86_64'
cmake -S openvino-whisper-helper -B build/openvino-helper -DOpenVINO_DIR="$sdk/runtime/cmake" -DOpenVINOGenAI_DIR="$sdk/runtime/cmake" -DOPENVINO_WHISPER_JSON_INCLUDE="$sdk/samples/cpp/thirdparty/nlohmann_json/single_include"
cmake --build build/openvino-helper --config Release
ctest --test-dir build/openvino-helper --build-config Release --output-on-failure
```

Place the built `.exe` in `frontend/src-tauri/binaries/` with the target-triple suffix. Copy the same runtime DLL set that CI copies into `frontend/src-tauri/binaries/openvino-runtime/`. The application refuses to start the helper if this bundled runtime directory is missing. The helper may also be selected with `MEETILY_OPENVINO_HELPER` for local diagnosis; installed users do not need this setting. Build prerequisites are MSVC C++ tools and CMake, as for other native Windows components.

## Protocol and diagnostics

Protocol version 1 uses `uint32_le` header length, UTF-8 JSON header, and binary little-endian IEEE-754 `f32` audio bytes for `transcribe` only. Response is a length-prefixed JSON object. Requests are serialized and have unique IDs. Supported operations are `hello`, `health`, `probe`, `load_model`, `transcribe`, `unload_model`, and `shutdown`. The helper writes protocol bytes only to stdout and diagnostics to stderr. Header, response, sample count, and path lengths are bounded.

Errors distinguish an unavailable NPU, missing/corrupt model, model/runtime incompatibility, compilation failure, helper failure, and malformed IPC. Technical diagnostics should not include audio or transcript text. Relevant metrics are runtime/model revision, device, model load time, audio duration, inference time, and real-time factor (`inference_time / audio_duration`).

## Hardware acceptance and benchmark record

Automated CI runners have no NPU. On the reference Windows 11 Intel Core Ultra 7 265U machine:

1. Verify the model manager reports `NPU` and the correct runtime version.
2. Download Small INT8 and transcribe German speech in both live recording and imported/retranscribed media. Confirm the output remains German with `de` or automatic transcription, and that Meetily persists the transcript.
3. Watch Task Manager > Performance > NPU during inference. Confirm NPU activity and no silent CPU/GPU execution.
4. Transcribe multiple chunks and confirm one helper/model load per session. Repeat after restart to assess compilation cache reuse.
5. Benchmark the approximately 32-minute reference audio, including the warm inference and total wall time.

| Field | Result |
| --- | --- |
| Hardware / OS / Intel NPU driver | To record on reference machine |
| OpenVINO / model | 2026.4.0 / Small INT8 pinned revision above |
| Audio duration | To measure |
| Initial model load/compile | To measure |
| Warm model inference time | To measure |
| Total time / aggregate RTF | To measure |
| Observed NPU / CPU / GPU utilization | To observe |
| German quality notes | To record |

A target aggregate RTF below 1 after warm-up is a performance goal, not a reason to substitute another device. A clean-user-account installation test is also required before release.

## Licenses

OpenVINO Runtime, GenAI, Tokenizers, and the official converted Whisper artifacts are distributed under Apache-2.0 according to their official repositories/model cards. The original OpenAI Whisper models use the MIT license. The bundled archive's Runtime, GenAI, and Tokenizers notices are copied into `openvino-runtime/`; the model sources and revisions are listed above. The SDK's `nlohmann/json` header is used only at build time under the MIT license included with the official SDK archive.
