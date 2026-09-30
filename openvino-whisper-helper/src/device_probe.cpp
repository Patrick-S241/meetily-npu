#include "device_probe.hpp"

#include <openvino/openvino.hpp>

namespace meetily::openvino_whisper {
DeviceProbe probe_npu() {
    ov::Core core;
    DeviceProbe probe;
    probe.available_devices = core.get_available_devices();
    probe.openvino_version = ov::get_openvino_version().buildNumber;
    for (const auto& device : probe.available_devices) {
        if (device == "NPU") {
            probe.npu_available = true;
            try { probe.npu_name = core.get_property(device, ov::device::full_name); }
            catch (const ov::Exception&) { probe.npu_name = "Intel NPU"; }
            break;
        }
    }
    return probe;
}
} // namespace meetily::openvino_whisper
