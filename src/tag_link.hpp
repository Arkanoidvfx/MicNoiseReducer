#pragma once
#include "audio.hpp"
#include <stdexcept>

namespace mic {
// Same-session IPC. One producer (the existing TAG ownership mutex), one permanent
// driver host. Acknowledged writes add no playback queue or extra reserve.
struct TagPacket {
    unsigned version, command, frames, running, capacity, gaps;
    int result;
    float samples[16384];
};
class TagLink {
    HANDLE mapping_=nullptr;
public:
    HANDLE mutex=nullptr, request=nullptr, response=nullptr;
    TagPacket* packet=nullptr;
    explicit TagLink(bool create,bool headphones=false) {
        try {
            const std::wstring prefix=headphones?L"Local\\MicNoize.HeadphoneLink":L"Local\\MicNoize.TagLink";
            mutex=CreateMutexW(nullptr,FALSE,(prefix+L".Lock").c_str());
            request=CreateEventW(nullptr,FALSE,FALSE,(prefix+L".Request").c_str());
            response=CreateEventW(nullptr,FALSE,FALSE,(prefix+L".Response").c_str());
            mapping_=create?CreateFileMappingW(INVALID_HANDLE_VALUE,nullptr,PAGE_READWRITE,0,sizeof(TagPacket),(prefix+L".v1").c_str()):
                OpenFileMappingW(FILE_MAP_ALL_ACCESS,FALSE,(prefix+L".v1").c_str());
            if(mapping_) packet=static_cast<TagPacket*>(MapViewOfFile(mapping_,FILE_MAP_ALL_ACCESS,0,0,sizeof(TagPacket)));
            if(!mutex || !request || !response || !packet) throw std::runtime_error("TAG background host unavailable");
        } catch(...) {close();throw;}
    }
    ~TagLink(){close();}
    TagLink(const TagLink&)=delete;
    TagLink& operator=(const TagLink&)=delete;
    bool lock(DWORD ms) {
        auto r=WaitForSingleObject(mutex,ms);
        return r==WAIT_OBJECT_0 || r==WAIT_ABANDONED;
    }
    void unlock(){ReleaseMutex(mutex);}
private:
    void close() {
        if(packet) UnmapViewOfFile(packet);
        if(mapping_) CloseHandle(mapping_);
        if(mutex) CloseHandle(mutex);
        if(request) CloseHandle(request);
        if(response) CloseHandle(response);
    }
};
// False while the core component is still downloading: bin/mic_tag_host.exe arrives with it.
inline bool tagHostInstalled() noexcept {
    try {return std::filesystem::is_regular_file(projectRoot()/L"bin/mic_tag_host.exe");}
    catch(...) {return false;}
}
inline HANDLE tagHostProcess() {
    HANDLE map=OpenFileMappingW(FILE_MAP_READ,FALSE,L"Local\\MicNoize.TagLink.v1");
    if(map) {CloseHandle(map);return nullptr;}
    const auto exe=projectRoot()/L"bin/mic_tag_host.exe";
    std::wstring command=L"\""+exe.wstring()+L"\"";
    STARTUPINFOW startup{};startup.cb=sizeof(startup);startup.dwFlags=STARTF_USESHOWWINDOW;startup.wShowWindow=SW_HIDE;
    PROCESS_INFORMATION process{};
    if(!CreateProcessW(exe.c_str(),command.data(),nullptr,nullptr,FALSE,CREATE_NO_WINDOW,nullptr,exe.parent_path().c_str(),&startup,&process))
        throw std::runtime_error("Не удалось запустить фоновый процесс виртуального микрофона");
    CloseHandle(process.hThread);return process.hProcess;
}
inline void ensureTagHost() {
    HANDLE process=tagHostProcess();
    // The mapping is published after TAG opens; wait for protocol initialization too.
    for(unsigned i=0;i<200;++i) {
        HANDLE map=OpenFileMappingW(FILE_MAP_READ,FALSE,L"Local\\MicNoize.TagLink.v1");
        if(map) {
            const auto p=static_cast<const TagPacket*>(MapViewOfFile(map,FILE_MAP_READ,0,0,sizeof(TagPacket)));
            HANDLE lock=OpenMutexW(SYNCHRONIZE|MUTEX_MODIFY_STATE,FALSE,L"Local\\MicNoize.TagLink.Lock");
            bool ready=false;
            if(p && lock) {
                const auto wait=WaitForSingleObject(lock,10);
                if(wait==WAIT_OBJECT_0 || wait==WAIT_ABANDONED){ready=p->version==1;ReleaseMutex(lock);}
            }
            if(lock)CloseHandle(lock);if(p)UnmapViewOfFile(p);CloseHandle(map);
            if(ready){if(process)CloseHandle(process);return;}
        }
        // A second launcher can exit because another host is still initializing.
        if(process && WaitForSingleObject(process,10)==WAIT_OBJECT_0) {CloseHandle(process);process=nullptr;}
        if(!process) Sleep(10);
    }
    if(process) CloseHandle(process);
    throw std::runtime_error("TAG background host failed; see results/tag-host.log");
}
class TagClient {
    TagLink link_;
    bool running_=false,rejected_=false;
    unsigned capacity_=0,gaps_=0;
    // Returns the host's result code; throws only on transport failures.
    int command(unsigned op,const float* samples=nullptr,unsigned frames=0) {
        if(!link_.lock(500)) throw std::runtime_error("TAG host lock timeout");
        if(link_.packet->version!=1) {link_.unlock();throw std::runtime_error("TAG host protocol mismatch");}
        ResetEvent(link_.response);
        link_.packet->command=0;link_.packet->frames=frames;link_.packet->result=-1;
        if(frames) std::copy_n(samples,frames,link_.packet->samples);
        link_.packet->command=op; // Publish only a complete block, including after an abandoned mutex.
        link_.unlock();SetEvent(link_.request);
        if(WaitForSingleObject(link_.response,500)!=WAIT_OBJECT_0) throw std::runtime_error("TAG host stopped responding");
        if(!link_.lock(500)) throw std::runtime_error("TAG host response timeout");
        const auto result=link_.packet->result;link_.unlock();
        return result;
    }
public:
    TagClient():link_(false){command(1);handleEvent();}
    ~TagClient(){try{command(3);}catch(...){}}
    void handleEvent() {
        if(!link_.lock(500)) throw std::runtime_error("TAG host status timeout");
        running_=link_.packet->running!=0;capacity_=link_.packet->capacity;gaps_=link_.packet->gaps;
        link_.unlock();
    }
    bool running() const{return running_;}
    unsigned capacity() const{return capacity_;}
    unsigned driverGaps() const{return gaps_;}
    // False: the host's 50 ms producer watchdog dropped us and already covered the gap with
    // silence, so this block (which carries the clock debt of the stall) is discarded and the
    // producer re-registers. Two consecutive rejections are a real protocol error.
    bool write(const float* samples,unsigned frames) {
        if(frames>16384) throw std::runtime_error("TAG IPC block exceeds allocation");
        if(command(2,samples,frames)==0) {rejected_=false;return true;}
        if(rejected_) throw std::runtime_error("TAG host rejected audio; restart processing");
        rejected_=true;command(1);handleEvent();return false;
    }
};
class HeadphoneClient {
    TagLink link_{false,true};
    bool running_=false;unsigned capacity_=0;
    void command(unsigned op,float* left=nullptr,float* right=nullptr,unsigned frames=0) {
        if(frames>8192)throw std::runtime_error("Headphone IPC block too large");
        if(!link_.lock(100))throw std::runtime_error("Headphone host lock timeout");
        if(link_.packet->version!=1){link_.unlock();throw std::runtime_error("Update TAG host for headphones");}
        ResetEvent(link_.response);link_.packet->frames=frames;link_.packet->result=-1;link_.packet->command=op;
        link_.unlock();SetEvent(link_.request);
        if(WaitForSingleObject(link_.response,500)!=WAIT_OBJECT_0)throw std::runtime_error("Headphone host response timeout");
        if(!link_.lock(100))throw std::runtime_error("Headphone response lock timeout");
        const auto& p=*link_.packet;const int result=p.result;
        running_=p.running!=0;capacity_=p.capacity;
        if(!result && frames){for(unsigned i=0;i<frames;++i){left[i]=p.samples[i*2];right[i]=p.samples[i*2+1];}}
        link_.unlock();
        if(result)throw std::runtime_error("Headphone host rejected request; see results/tag-headphones.log");
    }
public:
    HeadphoneClient(){command(1);}
    ~HeadphoneClient(){try{command(3);}catch(...){}}
    void handleEvent(){command(4);}
    bool running() const{return running_;}
    unsigned capacity() const{return capacity_;}
    void read(float* left,float* right,unsigned n){command(2,left,right,n);}
};
}
