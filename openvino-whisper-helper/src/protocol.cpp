#include "protocol.hpp"

#include <array>
#include <cmath>
#include <cstring>
#include <nlohmann/json.hpp>
#include <utility>

namespace meetily::openvino_whisper {
namespace {
using json = nlohmann::json;

bool read_exact(std::istream& input, char* destination, std::size_t count) {
    input.read(destination, static_cast<std::streamsize>(count));
    return input.gcount() == static_cast<std::streamsize>(count);
}
bool read_u32_le(std::istream& input, std::uint32_t& value) {
    std::array<unsigned char, 4> bytes{};
    if (!read_exact(input, reinterpret_cast<char*>(bytes.data()), bytes.size())) return false;
    value = static_cast<std::uint32_t>(bytes[0]) | (static_cast<std::uint32_t>(bytes[1]) << 8) | (static_cast<std::uint32_t>(bytes[2]) << 16) | (static_cast<std::uint32_t>(bytes[3]) << 24);
    return true;
}
void write_u32_le(std::ostream& output, std::uint32_t value) {
    const std::array<unsigned char, 4> bytes{{static_cast<unsigned char>(value & 0xff), static_cast<unsigned char>((value >> 8) & 0xff), static_cast<unsigned char>((value >> 16) & 0xff), static_cast<unsigned char>((value >> 24) & 0xff)}};
    output.write(reinterpret_cast<const char*>(bytes.data()), bytes.size());
}
bool required_uint(const json& object, const char* key, std::uint64_t& value) {
    const auto it = object.find(key);
    if (it == object.end() || !it->is_number_unsigned()) return false;
    value = it->get<std::uint64_t>();
    return true;
}
bool optional_string(const json& object, const char* key, std::string& value) {
    const auto it = object.find(key);
    if (it == object.end()) return true;
    if (!it->is_string()) return false;
    value = it->get<std::string>();
    return true;
}
json response_json(const Response& response) {
    json result{{"protocol", kProtocolVersion}, {"id", response.id}, {"ok", response.ok}};
    if (response.ok) {
        result["text"] = response.text; result["model"] = response.model; result["device"] = response.device; result["inference_ms"] = response.inference_ms;
        if (!response.npu_name.empty()) result["npu_name"] = response.npu_name;
        if (!response.openvino_version.empty()) result["openvino_version"] = response.openvino_version;
        if (!response.available_devices.empty()) result["available_devices"] = response.available_devices;
    } else { result["error_code"] = response.error_code; result["error"] = response.error; }
    return result;
}
} // namespace

bool read_request(std::istream& input, Request& request, ProtocolError& error) {
    std::uint32_t header_size = 0;
    if (!read_u32_le(input, header_size)) { error = {"IPC_EOF", "stdin closed before a complete frame header"}; return false; }
    if (header_size == 0 || header_size > kMaxHeaderBytes) { error = {"IPC_PROTOCOL_ERROR", "request header length is outside the allowed range"}; return false; }
    std::string header(header_size, '\0');
    if (!read_exact(input, header.data(), header.size())) { error = {"IPC_EOF", "stdin closed inside request header"}; return false; }
    const json object = json::parse(header, nullptr, false);
    if (object.is_discarded() || !object.is_object()) { error = {"IPC_PROTOCOL_ERROR", "request header is not a JSON object"}; return false; }
    std::uint64_t protocol = 0, id = 0; std::string op;
    if (!required_uint(object, "protocol", protocol) || protocol != kProtocolVersion || !required_uint(object, "id", id) || !optional_string(object, "op", op) || op.empty()) { error = {"IPC_PROTOCOL_ERROR", "request requires protocol 1, numeric id, and string op"}; return false; }
    request = {}; request.id = id; request.op = std::move(op);
    if (!optional_string(object, "model_path", request.model_path) || !optional_string(object, "model_id", request.model_id) || !optional_string(object, "cache_dir", request.cache_dir) || !optional_string(object, "language", request.language) || !optional_string(object, "task", request.task)) { error = {"IPC_PROTOCOL_ERROR", "request contains an invalid string field"}; return false; }
    if (request.task != "transcribe" && request.task != "translate") { error = {"IPC_PROTOCOL_ERROR", "task must be transcribe or translate"}; return false; }
    if (request.op != "transcribe") return true;
    std::uint64_t sample_rate = 0, sample_count = 0;
    if (!required_uint(object, "sample_rate", sample_rate) || !required_uint(object, "sample_count", sample_count) || sample_rate != 16000 || sample_count > kMaxAudioSamples) { error = {"IPC_PROTOCOL_ERROR", "transcribe requires 16000 Hz and a bounded sample_count"}; return false; }
    request.sample_rate = static_cast<std::uint32_t>(sample_rate); request.sample_count = static_cast<std::uint32_t>(sample_count); request.audio.resize(request.sample_count);
    if (!read_exact(input, reinterpret_cast<char*>(request.audio.data()), request.audio.size() * sizeof(float))) { error = {"IPC_EOF", "stdin closed inside transcribe audio payload"}; return false; }
    for (float sample : request.audio) if (!std::isfinite(sample) || sample < -1.1F || sample > 1.1F) { error = {"IPC_PROTOCOL_ERROR", "audio samples must be finite normalized f32 values"}; return false; }
    return true;
}
std::string serialize_response(const Response& response) {
    // Native exception strings may contain invalid UTF-8 from Windows paths.
    // Keep the IPC frame valid instead of terminating the helper on serialization.
    return response_json(response).dump(-1, ' ', false, json::error_handler_t::replace);
}
bool write_response(std::ostream& output, const Response& response, ProtocolError& error) {
    const std::string serialized = serialize_response(response);
    if (serialized.size() > kMaxResponseBytes) { error = {"IPC_PROTOCOL_ERROR", "response exceeds protocol limit"}; return false; }
    write_u32_le(output, static_cast<std::uint32_t>(serialized.size())); output.write(serialized.data(), static_cast<std::streamsize>(serialized.size())); output.flush();
    if (!output) { error = {"IPC_EOF", "stdout closed while writing response"}; return false; }
    return true;
}
Response error_response(std::uint64_t id, std::string code, std::string message) { Response response; response.id = id; response.error_code = std::move(code); response.error = std::move(message); return response; }
} // namespace meetily::openvino_whisper
