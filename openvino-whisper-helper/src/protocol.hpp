#pragma once

#include <cstdint>
#include <istream>
#include <ostream>
#include <string>
#include <vector>

namespace meetily::openvino_whisper {
constexpr std::uint32_t kProtocolVersion = 1;
constexpr std::uint32_t kMaxHeaderBytes = 64 * 1024;
constexpr std::uint32_t kMaxResponseBytes = 1024 * 1024;
constexpr std::uint32_t kMaxAudioSamples = 28'800'000; // 30 minutes at 16 kHz
struct ProtocolError { std::string code; std::string message; };
struct Request { std::uint64_t id = 0; std::string op, model_path, model_id, cache_dir, language, task = "transcribe"; std::uint32_t sample_rate = 0, sample_count = 0; std::vector<float> audio; };
struct Response { std::uint64_t id = 0; bool ok = false; std::string error_code, error, text, model, device, npu_name, openvino_version; std::uint64_t inference_ms = 0; std::vector<std::string> available_devices; };
bool read_request(std::istream& input, Request& request, ProtocolError& error);
bool write_response(std::ostream& output, const Response& response, ProtocolError& error);
std::string serialize_response(const Response& response);
Response error_response(std::uint64_t id, std::string code, std::string message);
} // namespace meetily::openvino_whisper
