#include "device_probe.hpp"
#include "protocol.hpp"
#include "whisper_engine.hpp"

#include <algorithm>
#include <chrono>
#include <exception>
#include <iostream>
#include <utility>

namespace mw = meetily::openvino_whisper;
namespace {
mw::Response probe_response(std::uint64_t id) {
    const auto probe = mw::probe_npu();
    // GenAI runs Whisper's tokenizer and detokenizer on CPU even when the
    // encoder and decoder are compiled for the explicitly selected NPU.
    if (std::find(probe.available_devices.begin(), probe.available_devices.end(), "CPU") == probe.available_devices.end())
        return mw::error_response(id, "RUNTIME_INCOMPLETE", "Bundled OpenVINO CPU plugin is missing. GenAI requires it for tokenization; Whisper inference remains on NPU.");
    if (!probe.npu_available) return mw::error_response(id, "NPU_NOT_AVAILABLE", "Intel NPU is unavailable. Update the Intel NPU driver through its OEM or Intel-supported channel.");
    mw::Response response; response.id = id; response.ok = true; response.device = "NPU"; response.npu_name = probe.npu_name; response.openvino_version = probe.openvino_version; response.available_devices = probe.available_devices; return response;
}
std::string classify_exception(const std::exception& e) {
    const std::string message = e.what();
    if (message.find("NPU_NOT_AVAILABLE") != std::string::npos) return "NPU_NOT_AVAILABLE";
    if (message.find("opset") != std::string::npos || message.find("unsupported") != std::string::npos || message.find("Cannot create") != std::string::npos) return "MODEL_RUNTIME_INCOMPATIBLE";
    if (message.find("model directory") != std::string::npos || message.find("MODEL_NOT_FOUND") != std::string::npos) return "MODEL_NOT_FOUND";
    return "NPU_COMPILE_FAILED";
}
} // namespace

int main() {
    std::ios::sync_with_stdio(false);
    mw::WhisperEngine engine;
    for (;;) {
        mw::Request request; mw::ProtocolError error;
        if (!mw::read_request(std::cin, request, error)) {
            if (error.code != "IPC_EOF") { mw::ProtocolError ignored; mw::write_response(std::cout, mw::error_response(0, error.code, error.message), ignored); }
            std::cerr << "openvino-whisper-helper: " << error.code << ": " << error.message << '\n';
            return error.code == "IPC_EOF" ? 0 : 2;
        }
        mw::Response response;
        try {
            if (request.op == "hello") { response.id = request.id; response.ok = true; response.device = "NPU"; response.openvino_version = OPENVINO_WHISPER_OPENVINO_VERSION; }
            else if (request.op == "health") { response.id = request.id; response.ok = true; response.device = "NPU"; response.model = engine.model_id(); response.openvino_version = OPENVINO_WHISPER_OPENVINO_VERSION; }
            else if (request.op == "probe") response = probe_response(request.id);
            else if (request.op == "load_model") { const auto probe = probe_response(request.id); if (!probe.ok) response = probe; else { engine.load(request.model_path, request.model_id, request.cache_dir); response.id = request.id; response.ok = true; response.model = engine.model_id(); response.device = "NPU"; } }
            else if (request.op == "unload_model") { engine.unload(); response.id = request.id; response.ok = true; response.device = "NPU"; }
            else if (request.op == "transcribe") { const auto start = std::chrono::steady_clock::now(); const auto text = engine.transcribe(request.audio, request.language, request.task); response.id = request.id; response.ok = true; response.text = text; response.model = engine.model_id(); response.device = "NPU"; response.inference_ms = std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now() - start).count(); }
            else if (request.op == "shutdown") { engine.unload(); response.id = request.id; response.ok = true; response.device = "NPU"; mw::ProtocolError write_error; mw::write_response(std::cout, response, write_error); return 0; }
            else response = mw::error_response(request.id, "IPC_PROTOCOL_ERROR", "unsupported operation");
        } catch (const std::exception& e) {
            std::cerr << "openvino-whisper-helper: request " << request.id << " failed: " << e.what() << '\n';
            const auto code = request.op == "transcribe" ? "TRANSCRIPTION_FAILED" : classify_exception(e);
            std::string message = "OpenVINO NPU operation failed. Check the model/runtime compatibility and Intel NPU driver.";
            if (request.op == "load_model") {
                message = e.what();
                if (message.size() > 2048) message.resize(2048);
            }
            response = mw::error_response(request.id, code, std::move(message));
        }
        mw::ProtocolError write_error;
        if (!mw::write_response(std::cout, response, write_error)) { std::cerr << "openvino-whisper-helper: " << write_error.message << '\n'; return 3; }
    }
}
