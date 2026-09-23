#pragma once
#include "audio.hpp"
#include <winternl.h>
#include <mmreg.h>
#include <ks.h>
#include <ksmedia.h>
#include <winioctl.h>
#include <cmath>
#include <fstream>
#include <stdexcept>
#define _Debug 0
#define _BuildMode 2
#include <driver/DriverApi.h>
#include <apilib/userapi.h>
#undef _BuildMode
#undef _Debug

namespace mic {
// Only the documented user-mode TAG interface is used; the signed driver is unchanged.
class TagOutput {
    HMODULE module_=nullptr;
    ThinAudioGateway::TagDriver* driver_=nullptr;
    ThinAudioGateway::TagPipe* pipe_=nullptr;
    bool ownsDriver_=true;
    void release() {
        if(pipe_) pipe_->Delete();
        if(driver_ && ownsDriver_) driver_->Delete();
        if(module_) FreeLibrary(module_);
    }
    static void ok(HRESULT hr,const char* where) {
        if(FAILED(hr)) {
            char s[180]; sprintf_s(s,"%s: HRESULT 0x%08lX",where,static_cast<unsigned long>(hr));
            throw std::runtime_error(s);
        }
    }
    static void logLineFailure(const char* action,unsigned id,HRESULT hr) noexcept {
        try {
            const auto dir=projectRoot()/L"results";std::filesystem::create_directories(dir);
            std::ofstream(dir/L"tag-host.log",std::ios::app)<<"TAG "<<action<<" line "<<id
                <<": HRESULT 0x"<<std::hex<<static_cast<unsigned long>(hr)<<'\n';
        } catch(...) {}
    }
public:
    explicit TagOutput(const std::filesystem::path& root,HANDLE event,bool capture=true,TagOutput* shared=nullptr) {
        using namespace ThinAudioGateway;
        try {
            if(shared){driver_=shared->driver_;ownsDriver_=false;}else{
            module_=LoadLibraryExW((root/L"apidll/x64/tagapi.dll").c_str(),nullptr,LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);
            if(!module_) throw std::runtime_error("Cannot load TAG API DLL");
            using Create=HRESULT (__stdcall*)(void**,const GUID*);
            auto create=reinterpret_cast<Create>(GetProcAddress(module_,"Driver_Create"));
            if(!create) throw std::runtime_error("TAG Driver_Create export missing");
            const GUID product={0x4d699d4a,0x65a5,0x40ec,{0x98,0x75,0x8e,0x6d,0x5f,0xc0,0x1e,0x0c}};
            ok(create(reinterpret_cast<void**>(&driver_),&product),"TAG create interface");
            ok(driver_->Find(),"Виртуальный аудиодрайвер не установлен");
            ok(driver_->Open(),"TAG open driver");
            }
            DriverInfo info{}; ok(driver_->GetInfo(info),"TAG driver info");
            if(info.VerMajor!=2 || info.NumLines>128) throw std::runtime_error("Unsupported TAG driver version/line count");
            std::vector<VirtualLineDesc> lines(std::max(1u,info.NumLines)); unsigned count=0;
            ok(driver_->GetLineList(lines.data(),static_cast<unsigned>(lines.size()*sizeof(VirtualLineDesc)),count),"TAG line list");
            if(count>lines.size()) throw std::runtime_error("Invalid TAG line count");
            // The demo driver holds a fixed number of lines (three here) and ships default
            // "TAG Microphone" / "TAG Speakers" lines nothing of ours uses; with them present
            // the headphone line is refused (0x80070044). Remove them; the list is re-read.
            bool removed=false;
            for(unsigned i=0;i<count;++i)
                if(!wcsncmp(lines[i].KsName,L"TAG ",4)) {
                    if(const auto hr=driver_->DeleteLine(lines[i].Id);FAILED(hr)) logLineFailure("delete default",lines[i].Id,hr);
                    else removed=true;
                }
            if(removed) {
                ok(driver_->GetLineList(lines.data(),static_cast<unsigned>(lines.size()*sizeof(VirtualLineDesc)),count),"TAG line list");
                if(count>lines.size()) throw std::runtime_error("Invalid TAG line count");
            }
            unsigned id=0,fallback=0,next=1;
            for(unsigned i=0;i<count;++i) {
                next=std::max(next,lines[i].Id+1);
                const auto* wanted=capture?L"MicNoize Microphone":L"MicNoize Headphones";
                if(!id && lines[i].Capture==capture && !wcscmp(lines[i].KsName,wanted)) id=lines[i].Id;
                if(capture && !fallback && lines[i].Capture) fallback=lines[i].Id;
            }
            bool newLine=id!=0;
            if(!id) {
                VirtualLineDesc line{}; line.Id=next; line.Capture=capture; line.Type=capture?VLT_Microphone:VLT_Headphones;
                wcscpy_s(line.KsName,capture?L"MicNoize Microphone":L"MicNoize Headphones");
                wcscpy_s(line.EpName,capture?L"Mic Noize":L"Mic Noize Headphones");
                const auto hr=driver_->CreateLine(line);
                if(SUCCEEDED(hr)){id=next;newLine=true;}
                else if(capture && fallback){id=fallback;logLineFailure("create",next,hr);}
                else {
                    // Name the lines the driver already has: a refusal is otherwise just a code.
                    char s[160]; sprintf_s(s,"TAG create line %u: HRESULT 0x%08lX; driver has %u line(s):",next,static_cast<unsigned long>(hr),count);
                    std::string m=s;
                    for(unsigned i=0;i<count;++i){
                        char n[sizeof lines[i].KsName]; size_t conv=0; wcstombs_s(&conv,n,lines[i].KsName,_TRUNCATE);
                        sprintf_s(s," [id %u %s type %d \"%s\"]",lines[i].Id,lines[i].Capture?"capture":"render",static_cast<int>(lines[i].Type),n);
                        m+=s;
                    }
                    throw std::runtime_error(m);
                }
            }
            if(capture && newLine) {
                constexpr wchar_t legacy[]=L"MicNoiseReducer"; // Legacy name, migration only.
                for(unsigned i=0;i<count;++i)
                    if(!wcsncmp(lines[i].KsName,legacy,std::size(legacy)-1))
                        if(const auto hr=driver_->DeleteLine(lines[i].Id);FAILED(hr)) logLineFailure("delete legacy",lines[i].Id,hr);
            }
            ok(driver_->CreatePipe(pipe_,id,capture),"TAG open audio pipe");
            // 32-bit integer PCM; conversion from NVIDIA float happens only at the output.
            FormatRangeDesc range{48000,48000,32,32,2,2,4,4};
            WaveFormatDesc format{48000,32,2,4,SPEAKER_FRONT_LEFT|SPEAKER_FRONT_RIGHT};
            ok(pipe_->SetFormatRange(range),"TAG set format range");
            ok(pipe_->SetDefaultFormat(format),"TAG set default format");
            ok(pipe_->SetNotificationEvent(event),"TAG connect microphone");
        } catch(...) { release(); throw; }
    }
    ~TagOutput(){release();}
    TagOutput(const TagOutput&)=delete;
    TagOutput& operator=(const TagOutput&)=delete;
    bool handleEvent() {
        auto& c=pipe_->GetCommonData();
        if(!(c.Drv.FI.Flags&ThinAudioGateway::FilterFlags::WaitingForHost)) return false;
        if(c.Drv.FI.HasStream) {
            const auto& f=c.Drv.SI.Fmt;
            if(f.Format.nSamplesPerSec!=48000 || f.Format.nChannels!=2 || f.Format.wBitsPerSample!=32 ||
               f.Format.nBlockAlign!=8 || (f.Format.wFormatTag!=WAVE_FORMAT_PCM &&
               (f.Format.wFormatTag!=WAVE_FORMAT_EXTENSIBLE || !IsEqualGUID(f.SubFormat,KSDATAFORMAT_SUBTYPE_PCM)))) {
                pipe_->Notify(static_cast<NTSTATUS>(0xc000000d));
                throw std::runtime_error("TAG client format must be 48 kHz stereo PCM32");
            }
        }
        ok(pipe_->Notify(0),"TAG acknowledge stream state");
        return true;
    }
    bool running() const {
        const auto& c=pipe_->GetCommonData();
        return c.Drv.FI.HasStream && c.Drv.SI.State==KSSTATE_RUN && c.Drv.SI.BufferFrames && c.Host.Buffer;
    }
    unsigned capacity() const {return pipe_->GetCommonData().Drv.SI.BufferFrames;}
    unsigned driverGaps() const {
        const auto& c=pipe_->GetCommonData(); return c.Drv.FI.DataOverflows+c.Drv.FI.DataUnderflows;
    }
    void read(float* left,float* right,unsigned n) {
        auto& c=pipe_->GetCommonData();
        const auto frames=c.Drv.SI.BufferFrames,pos=c.Host.DevicePos;
        if(pipe_->IsCapt() || !running() || pos>=frames || n>frames/2) throw std::runtime_error("Invalid TAG render read");
        MemoryBarrier();
        const auto in=static_cast<const int32_t*>(c.Host.Buffer);
        for(unsigned i=0;i<n;++i) {
            const auto p=((pos+i)%frames)*2;
            left[i]=static_cast<float>(in[p]/2147483648.0);
            right[i]=static_cast<float>(in[p+1]/2147483648.0);
        }
        ok(pipe_->AdvanceDevicePosition(n),"TAG advance render position");
    }
    void write(const float* samples,unsigned n) {
        auto& c=pipe_->GetCommonData();
        const auto frames=c.Drv.SI.BufferFrames, pos=c.Host.DevicePos;
        if(!running() || pos>=frames || n>frames/2) throw std::runtime_error("Invalid TAG buffer write");
        auto out=static_cast<int32_t*>(c.Host.Buffer);
        for(unsigned i=0;i<n;++i) {
            const auto v=static_cast<int32_t>(std::clamp(static_cast<double>(samples[i]),-1.0,1.0)*2147483647.0);
            const auto p=((pos+i)%frames)*2; out[p]=v; out[p+1]=v;
        }
        MemoryBarrier();
        ok(pipe_->AdvanceDevicePosition(n),"TAG advance capture position");
    }
};
}
