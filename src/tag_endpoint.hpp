#pragma once
#include <windows.h>
#include <endpointvolume.h>
#include <mmdeviceapi.h>
#include <atomic>
#include <memory>
#include <string>
#include <string_view>
#include "tag_protocol.hpp"

namespace mic {
inline ULONGLONG tagAwakeMilliseconds(){ULONGLONG value=0;QueryUnbiasedInterruptTime(&value);return value/10000;}
// Independent status channel. The existing audio .v1 packets are not reinterpreted.
struct TagEndpointStatus {
    unsigned version=2,bytes=sizeof(TagEndpointStatus),pid=0,line=0,ready=0;
    unsigned audioVersion=tagProtocolVersion,hostBuild=tagHostBuild;
    GUID hostId{};
    FILETIME processStarted{};
    ULONGLONG checkedAt=0;
    float compensation=1;
    wchar_t endpoint[512]{};
    char error[256]{};
};
// False means no status publisher. A dead publisher is returned with ready cleared.
bool readTagEndpointStatus(TagEndpointStatus& result);
bool tagEndpointHostAlive(const TagEndpointStatus& status);
// Device identity survives a Windows display-name change. Optional topology selects an exact own line.
bool tagDriverEndpoint(const std::wstring& endpoint,std::wstring_view topology={});
float holdTagEndpointLevel(IAudioEndpointVolume* level,bool unmute=true,const GUID* context=nullptr);
// Only pass the endpoint confirmed by driver/KS identity. Returns whether policy changed.
bool holdTagEndpointSharedMode(IMMDevice* endpoint);
class TagEndpointGuard {
    struct Impl;
    std::unique_ptr<Impl> impl_;
public:
    TagEndpointGuard(const std::wstring& driverInterface,const std::wstring& ksName,unsigned line,const GUID& host);
    ~TagEndpointGuard();
    TagEndpointGuard(const TagEndpointGuard&)=delete;
    TagEndpointGuard& operator=(const TagEndpointGuard&)=delete;
    bool ready() const;
    bool responsive() const;
};
}
