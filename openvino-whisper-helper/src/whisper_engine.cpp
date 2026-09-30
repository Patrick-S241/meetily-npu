#include "whisper_engine.hpp"

#include "device_probe.hpp"
#include <filesystem>
#include <openvino/genai/whisper_pipeline.hpp>
#include <openvino/openvino.hpp>
#include <stdexcept>

namespace meetily::openvino_whisper {
WhisperEngine::WhisperEngine() = default;
WhisperEngine::~WhisperEngine() = default;
void WhisperEngine::load(const std::string& model_path, const std::string& model_id, const std::string& cache_dir) {
    if (model_path.empty() || model_id.empty() || cache_dir.empty()) throw std::invalid_argument("load_model requires model_path, model_id, and cache_dir");
    if (!std::filesystem::is_directory(model_path)) throw std::runtime_error("model directory does not exist");
    if (!std::filesystem::is_directory(cache_dir)) throw std::runtime_error("cache directory does not exist");
    const DeviceProbe probe = probe_npu();
    if (!probe.npu_available) throw std::runtime_error("NPU_NOT_AVAILABLE: OpenVINO did not enumerate an NPU device");

    // Explicitly NPU. This never uses AUTO/HETERO and has no fallback target.
    ov::AnyMap properties{ov::cache_dir(cache_dir)};
    pipeline_ = std::make_unique<ov::genai::WhisperPipeline>(model_path, "NPU", properties);
    model_id_ = model_id;
}
void WhisperEngine::unload() { pipeline_.reset(); model_id_.clear(); }
bool WhisperEngine::loaded() const noexcept { return static_cast<bool>(pipeline_); }
const std::string& WhisperEngine::model_id() const noexcept { return model_id_; }
std::string WhisperEngine::transcribe(const std::vector<float>& audio, const std::string& language, const std::string& task) {
    if (!pipeline_) throw std::runtime_error("MODEL_NOT_FOUND: no model is loaded");
    auto config = pipeline_->get_generation_config();
    config.task = task;
    if (!language.empty()) config.language = "<|" + language + "|>";
    const auto result = pipeline_->generate(audio, config);
    if (result.texts.empty()) return {};
    return result.texts.front();
}
} // namespace meetily::openvino_whisper
