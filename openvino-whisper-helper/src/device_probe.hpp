#pragma once

#include <string>
#include <vector>

namespace meetily::openvino_whisper {
struct DeviceProbe { bool npu_available = false; std::string npu_name; std::string openvino_version; std::vector<std::string> available_devices; };
DeviceProbe probe_npu();
} // namespace meetily::openvino_whisper
