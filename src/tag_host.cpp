#include "tag.hpp"
#include "tag_link.hpp"
#include <avrt.h>
#include <fstream>
#include <chrono>
#include <memory>

int WINAPI wWinMain(HINSTANCE,HINSTANCE,PWSTR,int) {
    HANDLE owner=CreateMutexW(nullptr,FALSE,L"Local\\MicNoize.TagHost");
    if(!owner) return 1;
    if(GetLastError()==ERROR_ALREADY_EXISTS){CloseHandle(owner);return 0;}
    int result=0;
    try {
        struct Handle {HANDLE h;~Handle(){if(h)CloseHandle(h);}};
        Handle event{CreateEventW(nullptr,FALSE,FALSE,nullptr)};
        Handle timer{CreateWaitableTimerExW(nullptr,nullptr,CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,TIMER_ALL_ACCESS)};
        if(!event.h || !timer.h) throw std::runtime_error("TAG host event/timer creation failed");
        mic::TagOutput tag(mic::projectRoot()/L"vendor/tag-2.0.0.1903-demo",event.h);
        mic::TagLink link(true);
        mic::TagLink headphones(true,true);
        Handle headphoneEvent{CreateEventW(nullptr,FALSE,FALSE,nullptr)};
        if(!headphoneEvent.h)throw std::runtime_error("Headphone event failed");
        if(!headphones.lock(500))throw std::runtime_error("Headphone initialization lock");
        *headphones.packet={};headphones.packet->version=1;headphones.unlock();
        std::unique_ptr<mic::TagOutput> headphonePipe;
        std::array<float,8192> left{},right{};
        auto headphoneSeen=std::chrono::steady_clock::now();
        if(!link.lock(500)) throw std::runtime_error("TAG host initialization lock");
        *link.packet={};link.packet->version=1;link.unlock();
        DWORD task=0;HANDLE mmcss=AvSetMmThreadCharacteristicsW(L"Audio",&task);
        struct Priority {HANDLE h;~Priority(){if(h)AvRevertMmThreadCharacteristics(h);}} priority{mmcss};
        std::array<float,16384> silence{};
        bool connected=false,producing=false,wasRunning=false,retryLock=false;
        auto last=std::chrono::steady_clock::now(),lastProducer=last;
        mic::TagClock clock;
        HANDLE events[]={event.h,link.request,timer.h,headphones.request,headphoneEvent.h};
        for(;;) {
            const auto wait=WaitForMultipleObjects(5,events,FALSE,retryLock?2:(headphonePipe?1000:INFINITE));
            if(wait==WAIT_FAILED) throw std::runtime_error("TAG host wait failed");
            tag.handleEvent();
            const auto now=std::chrono::steady_clock::now();
            const bool headphoneRetry=!headphones.lock(0);
            if(!headphoneRetry) {
                auto& p=*headphones.packet;const unsigned op=p.command;p.command=0;
                try {
                    if(op)headphoneSeen=now;
                    if(op==1 && !headphonePipe)headphonePipe=std::make_unique<mic::TagOutput>(mic::projectRoot()/L"vendor/tag-2.0.0.1903-demo",headphoneEvent.h,false,&tag);
                    if(op==3 || (headphonePipe && now-headphoneSeen>std::chrono::seconds(2)))headphonePipe.reset();
                    if(headphonePipe)headphonePipe->handleEvent();
                    p.running=headphonePipe && headphonePipe->running();p.capacity=headphonePipe?headphonePipe->capacity():0;p.result=0;
                    if(op==2){
                        if(!headphonePipe || p.frames>8192 || (p.running && p.frames>p.capacity/2))throw std::runtime_error("Invalid headphone read size");
                        if(p.running)headphonePipe->read(left.data(),right.data(),p.frames);
                        else {left.fill(0);right.fill(0);}
                        for(unsigned i=0;i<p.frames;++i){p.samples[i*2]=left[i];p.samples[i*2+1]=right[i];}
                    }else if(op>4)throw std::runtime_error("Invalid headphone command");
                }catch(const std::exception& e){
                    p.result=1;p.running=0;headphonePipe.reset();
                    try{std::ofstream(mic::projectRoot()/L"results/tag-headphones.log",std::ios::app)<<e.what()<<'\n';}catch(...){}
                }
                headphones.unlock();if(op)SetEvent(headphones.response);
            }
            const bool running=tag.running();
            if(running!=wasRunning){
                clock={};last=now;wasRunning=running;producing=false;
                if(running) {
                    LARGE_INTEGER due{};due.QuadPart=-20000;
                    if(!SetWaitableTimer(timer.h,&due,2,nullptr,nullptr,FALSE)) throw std::runtime_error("TAG host timer start failed");
                } else CancelWaitableTimer(timer.h);
            }
            unsigned command=0;
            // A suspended/crashed UI must never hold the driver host hostage.
            retryLock=!link.lock(0);
            if(!retryLock) {
                auto& p=*link.packet;
                p.running=running;p.capacity=tag.capacity();p.gaps=tag.driverGaps();
                command=p.command;
                if(command) {
                p.result=0;p.command=0;
                if(command==1){connected=true;producing=false;lastProducer=now;}
                else if(command==3){connected=false;producing=false;clock={};}
                else if(command==2 && connected && p.frames<=16384) {
                    const bool finite=std::all_of(p.samples,p.samples+p.frames,[](float v){return std::isfinite(v);});
                    if(!finite || (running && p.frames>tag.capacity()/2)) p.result=1;
                    else if(running) {
                        // A synchronous handoff: samples are consumed before the producer
                        // returns. No second audio queue, no old tail after Stop/crash.
                        try {tag.write(p.samples,p.frames);}
                        catch(...) {p.result=1;}
                    }
                    lastProducer=now;last=now;clock={};producing=running;
                } else p.result=1;
                }
                link.unlock();
            }
            if(command) SetEvent(link.response);
            retryLock=retryLock || headphoneRetry;
            if(producing && now-lastProducer>std::chrono::milliseconds(50)) {
                producing=false;connected=false;clock={};last=now;
            }
            if(running && !producing) {
                const auto frames=clock.take(std::chrono::duration<double>(now-last).count(),0,std::min(tag.capacity()/2,16384u));
                if(frames) tag.write(silence.data(),frames);
            }
            last=now;
        }
    } catch(const std::exception& e) {
        try {
            const auto dir=mic::projectRoot()/L"results";std::filesystem::create_directories(dir);
            std::ofstream(dir/L"tag-host.log",std::ios::app)<<e.what()<<'\n';
        } catch(...) {}
        result=1;
    }
    CloseHandle(owner);return result;
}
