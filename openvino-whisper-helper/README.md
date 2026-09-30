# OpenVINO Whisper NPU helper

`openvino-whisper-helper` is Meetily's persistent Windows sidecar for local
OpenVINO GenAI Whisper transcription. The Whisper encoder and decoder target
`NPU`; they never request `AUTO`, CPU, GPU, HETERO, or another fallback device.
GenAI compiles the tokenizer and detokenizer on CPU, so the matching OpenVINO
CPU plugin must also be bundled.

## Version and build

The helper is pinned to the OpenVINO release family **2026.4** (Runtime, GenAI,
and Tokenizers from the same redistributable). CI must provide the validated
Windows x64 GenAI archive through `CMAKE_PREFIX_PATH`; the CMake project makes
no downloads and end users do not need an OpenVINO SDK, Python, or environment
variables.

```powershell
cmake -S openvino-whisper-helper -B build/openvino-helper -DCMAKE_PREFIX_PATH=C:/deps/openvino_genai/runtime/cmake -DOPENVINO_WHISPER_JSON_INCLUDE=C:/deps/openvino_genai/samples/cpp/thirdparty/nlohmann_json/single_include
cmake --build build/openvino-helper --config Release
ctest --test-dir build/openvino-helper --build-config Release --output-on-failure
```

For hardware-free protocol tests only:

```powershell
cmake -S openvino-whisper-helper -B build/openvino-helper-tests -DOPENVINO_WHISPER_BUILD_HELPER=OFF -DOPENVINO_WHISPER_JSON_INCLUDE=C:/deps/openvino_genai/samples/cpp/thirdparty/nlohmann_json/single_include
cmake --build build/openvino-helper-tests
ctest --test-dir build/openvino-helper-tests --output-on-failure
```

The packaged application must place the helper beside the matching OpenVINO
runtime DLLs/plugins, including the NPU plugin and GenAI/tokenizer components.

## Protocol

One request is read and one response is written at a time. stdout contains only
binary protocol frames; diagnostics are written to stderr.

Request: `u32 little-endian JSON byte length`, UTF-8 JSON header, then only for
`transcribe`, `sample_count * 4` little-endian IEEE-754 `f32` samples. Responses
are `u32 little-endian JSON byte length` followed by JSON. Protocol version is 1.

Supported operations: `hello`, `probe`, `load_model`, `transcribe`,
`unload_model`, and `shutdown`. `transcribe` accepts mono 16 kHz normalized
samples, optional ISO 639-1 language hints such as `de`, and optional `task`. The task defaults to `transcribe`; `translate` is accepted only when explicitly requested and produces English translation. The engine converts
the hint to the GenAI Whisper token (`<|de|>`) and passes the selected task to GenAI Whisper.

Frames are bounded: headers 64 KiB, responses 1 MiB, audio 28,800,000 samples
(30 minutes at 16 kHz). A malformed/truncated request terminates the helper
after emitting a bounded error where possible, preventing stream desynchronization.

## Runtime behavior

`probe` asks OpenVINO to enumerate devices and succeeds only when `NPU` is
present. `load_model` again probes, then constructs
`ov::genai::WhisperPipeline(model_path, "NPU", {ov::cache_dir(cache_dir)})`.
The pipeline stays loaded until `unload_model` or `shutdown`; the cache directory
is supplied by Meetily's application cache path. Models must be already present
in Meetily's application-controlled model storage.

The current helper does not download models. The owning Rust client must use the
pinned model manifest and validate files before `load_model`.

## Manual NPU validation

On a supported Windows 11 Intel system, send `probe`, load the multilingual
Whisper Small INT8 model, transcribe German audio with `language: "de"`, and
confirm Task Manager's NPU activity. Verify diagnostics and response field
`device` are `NPU`. An unavailable NPU returns `NPU_NOT_AVAILABLE`; it never
falls back to CPU or GPU.
