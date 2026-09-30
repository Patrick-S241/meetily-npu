#pragma once

#include <memory>
#include <string>
#include <vector>

namespace ov::genai { class WhisperPipeline; }
namespace meetily::openvino_whisper {
class WhisperEngine {
public:
    WhisperEngine();
    ~WhisperEngine();
    void load(const std::string& model_path, const std::string& model_id, const std::string& cache_dir);
    void unload();
    [[nodiscard]] bool loaded() const noexcept;
    [[nodiscard]] const std::string& model_id() const noexcept;
    std::string transcribe(const std::vector<float>& audio, const std::string& language, const std::string& task);
private:
    std::unique_ptr<ov::genai::WhisperPipeline> pipeline_;
    std::string model_id_;
};
} // namespace meetily::openvino_whisper
