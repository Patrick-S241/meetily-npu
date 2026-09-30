#include "protocol.hpp"

#include <cassert>
#include <cstring>
#include <sstream>

namespace mw = meetily::openvino_whisper;
namespace {
void append_u32(std::string& bytes, std::uint32_t value) { for (int i = 0; i != 4; ++i) bytes.push_back(static_cast<char>((value >> (i * 8)) & 0xff)); }
std::string frame(const std::string& json, const std::string& audio = {}) { std::string bytes; append_u32(bytes, static_cast<std::uint32_t>(json.size())); bytes += json; bytes += audio; return bytes; }
void test_valid_transcribe() {
    const float samples[] = {0.25F, -0.25F}; std::string raw(sizeof(samples), '\0'); std::memcpy(raw.data(), samples, sizeof(samples));
    std::istringstream input(frame(R"({"protocol":1,"id":7,"op":"transcribe","sample_rate":16000,"sample_count":2,"language":"de","task":"translate"})", raw));
    mw::Request request; mw::ProtocolError error; assert(mw::read_request(input, request, error)); assert(request.id == 7 && request.audio.size() == 2 && request.language == "de" && request.task == "translate");
    std::istringstream unicode(frame(R"({"protocol":1,"id":8,"op":"load_model","model_path":"C:\\Models\\u00e4"})")); assert(mw::read_request(unicode, request, error)); assert(request.model_path.find(std::string({static_cast<char>(0xC3), static_cast<char>(0xA4)})) != std::string::npos);
}
void test_rejects_version_and_rate() {
    std::istringstream old_version(frame(R"({"protocol":2,"id":1,"op":"hello"})")); mw::Request request; mw::ProtocolError error; assert(!mw::read_request(old_version, request, error)); assert(error.code == "IPC_PROTOCOL_ERROR");
    std::istringstream malformed(frame("{\"protocol\":1,")); assert(!mw::read_request(malformed, request, error)); assert(error.code == "IPC_PROTOCOL_ERROR");
    std::istringstream invalid_task(frame(R"({"protocol":1,"id":2,"op":"hello","task":"summarize"})")); assert(!mw::read_request(invalid_task, request, error)); assert(error.code == "IPC_PROTOCOL_ERROR");
    std::istringstream wrong_rate(frame(R"({"protocol":1,"id":1,"op":"transcribe","sample_rate":48000,"sample_count":0})")); assert(!mw::read_request(wrong_rate, request, error));
}
void test_rejects_missing_audio_and_oversize_header() {
    std::istringstream truncated(frame(R"({"protocol":1,"id":1,"op":"transcribe","sample_rate":16000,"sample_count":2})", "\0\0\0\0")); mw::Request request; mw::ProtocolError error; assert(!mw::read_request(truncated, request, error)); assert(error.code == "IPC_EOF");
    std::string oversized; append_u32(oversized, mw::kMaxHeaderBytes + 1); std::istringstream input(oversized); assert(!mw::read_request(input, request, error)); assert(error.code == "IPC_PROTOCOL_ERROR");
}
void test_response_serialization() {
    mw::Response probe; probe.id = 3; probe.ok = true; probe.device = "NPU"; probe.npu_name = "Intel NPU"; probe.available_devices = {"CPU", "NPU"}; const auto json = mw::serialize_response(probe); assert(json.find("\"available_devices\":[\"CPU\",\"NPU\"]") != std::string::npos);
    const auto error = mw::error_response(3, "MODEL_NOT_FOUND", "missing"); const auto error_json = mw::serialize_response(error); assert(error_json.find("MODEL_NOT_FOUND") != std::string::npos);
    const auto invalid = mw::error_response(4, "NPU_COMPILE_FAILED", std::string("path: ") + static_cast<char>(0xC3));
    const auto invalid_json = mw::serialize_response(invalid);
    assert(invalid_json.find("NPU_COMPILE_FAILED") != std::string::npos);
    assert(invalid_json.find("\xEF\xBF\xBD") != std::string::npos);
}
} // namespace
int main() { test_valid_transcribe(); test_rejects_version_and_rate(); test_rejects_missing_audio_and_oversize_header(); test_response_serialization(); }
