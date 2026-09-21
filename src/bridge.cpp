#include "bridge.h"
#include "audio.hpp"
#include "tag_link.hpp"
#include <shellapi.h>
#include <wtsapi32.h>
#include <cmath>
#include <memory>
#include <psapi.h>

struct Mnr {
    mic::Engine engine;
    mic::Headphones headphones;
    mic::Monitor monitor{engine};
    std::wstring outputRoute;
    std::jthread shell;
    HANDLE instance=nullptr;
    std::atomic<HWND> window{nullptr};
    std::atomic<unsigned> keys[13]{};
    std::atomic<unsigned> events{0},captured{0};
    std::atomic<unsigned> captureGeneration{0};
    std::atomic<bool> capturing{false};
    bool locked=false,suspended=false;
    NOTIFYICONDATAW tray{};
    UINT taskbar=0;
    ~Mnr() {
        if(shell.joinable()) {shell.request_stop(); if(auto w=window.load()) PostMessageW(w,WM_APP+4,0,0); shell.join();}
        if(instance) CloseHandle(instance);
    }
};
static void copy(const std::string& text,char* out,uint32_t capacity) {
    if(!out || !capacity) return;
    const auto count=std::min<size_t>(text.size(),capacity-1);
    memcpy(out,text.data(),count); out[count]=0;
}
static std::wstring string(const char* value,uint32_t length) {
    if(!value || length>32768) throw std::runtime_error("Invalid UTF-8 string");
    return mic::wide(std::string(value,length));
}
extern "C" Mnr* mnr_create(char* error,uint32_t capacity) {
    try {return new Mnr;} catch(const std::exception& e) {copy(e.what(),error,capacity);return nullptr;} catch(...) {copy("Engine creation failed",error,capacity);return nullptr;}
}
extern "C" void mnr_destroy(Mnr* p) {try {delete p;} catch(...) {}}
extern "C" int32_t mnr_start(Mnr* p,const char* input,uint32_t il,const char* output,uint32_t ol,int32_t version,uint32_t buffer,uint32_t period,int32_t graphs,float intensity,char* error,uint32_t cap) {
    try {
        mic::Config c; c.input=string(input,il); c.output=string(output,ol); c.version=version;
        c.bufferMs=buffer; c.periodMs=period; c.cudaGraphs=graphs; c.intensity=intensity;
        c.tag=c.output==L"TAG"; c.sdk=mic::projectRoot()/L"vendor/nvidia-afx-3.0.0";
        c.tagSdk=mic::projectRoot()/L"vendor/tag-2.0.0.1903-demo";
        p->monitor.stop();p->engine.start(c);p->outputRoute=c.output;return 1;
    } catch(const std::exception& e) {copy(e.what(),error,cap);p->engine.reportError(e.what());return 0;}
    catch(...) {copy("Start failed",error,cap);p->engine.state=5;return 0;}
}
extern "C" void mnr_stop(Mnr* p) {try {p->monitor.stop();p->engine.stop();} catch(...) {p->engine.state=5;}}
extern "C" int32_t mnr_headphones(Mnr* p,int32_t enabled,const char* output,uint32_t length,int32_t denoise,char* error,uint32_t cap) {
    try {
        if((enabled!=0 && enabled!=1)||(denoise!=0 && denoise!=1))throw std::runtime_error("Invalid headphone mode");
        if(enabled)p->headphones.start(string(output,length),denoise!=0);else p->headphones.stop();return 1;
    }catch(const std::exception& e){copy(e.what(),error,cap);return 0;}catch(...){copy("Headphone start failed",error,cap);return 0;}
}
extern "C" void mnr_headphone_controls(Mnr* p,float intensity,float volume,int32_t pitch,int32_t muted) {
    if(!std::isfinite(intensity)||intensity<0||intensity>2||!std::isfinite(volume)||volume<0||volume>1||pitch< -12||pitch>12)return;
    p->headphones.intensity=intensity;p->headphones.volume=volume;p->headphones.pitch=pitch;p->headphones.muted=muted!=0;
}
extern "C" int32_t mnr_headphone_state(Mnr* p,char* text,uint32_t cap) {
    try{copy(mic::utf8(p->headphones.message()),text,cap);return p->headphones.state;}catch(...){return 3;}
}
extern "C" int32_t mnr_monitor(Mnr* p,int32_t enabled,char* error,uint32_t cap) {
    try {
        if(enabled<0 || enabled>4)throw std::runtime_error("Invalid monitor mode");
        if(enabled)p->monitor.start(p->outputRoute,static_cast<uint8_t>(enabled-1));else p->monitor.stop();return 1;
    }
    catch(const std::exception& e){copy(e.what(),error,cap);return 0;}catch(...){copy("Monitor failed",error,cap);return 0;}
}
extern "C" int32_t mnr_monitor_state(Mnr* p,char* text,uint32_t cap) {
    try {copy(mic::utf8(p->monitor.message()),text,cap);return p->monitor.state.load();}catch(...){return 3;}
}
extern "C" int32_t mnr_phrase_state(Mnr* p,float* seconds) {*seconds=p->engine.stats.phraseSeconds;return p->engine.stats.phraseState;}
extern "C" int32_t mnr_discord_state(Mnr* p,char* text,uint32_t cap,int32_t* active) {
    *active=p->engine.stats.desktopSource;
    try {copy(mic::utf8(p->engine.desktopMessage()),text,cap);return p->engine.stats.desktopState;}catch(...){return 3;}
}
extern "C" void mnr_phrase_cancel(Mnr* p) {++p->engine.phraseCancel;}
extern "C" void mnr_rvc_settings(Mnr* p,uint32_t slot,int32_t pitch,uint32_t index,uint32_t chunkMs,uint32_t gain) {
    if(slot>65535 || pitch < -24 || pitch>24 || index>100 || gain<50 || gain>300 ||
       (chunkMs!=100 && chunkMs!=150 && chunkMs!=200 && chunkMs!=300 && chunkMs!=500)) return;
    p->engine.rvcConfig=mic::RvcSettings{slot,chunkMs,index,gain,pitch}.packed();
}
extern "C" void mnr_controls(Mnr* p,float volume,float boost,int32_t pitch,float intensity,int32_t muted,float slow,float fast,int32_t overload,float discordVolume,int32_t rvcEnabled) {
    if(!std::isfinite(volume)||!std::isfinite(boost)||!std::isfinite(intensity)||!std::isfinite(slow)||!std::isfinite(fast)||!std::isfinite(discordVolume)) return;
    p->engine.volume=std::clamp(volume,0.0f,1.0f); p->engine.boost=std::clamp(boost,1.0f,20.0f);
    p->engine.overload=overload!=0;
    p->engine.discordVolume=std::clamp(discordVolume,0.0f,1.0f);
    p->engine.rvcEnabled=rvcEnabled!=0;
    p->engine.pitch=std::clamp(pitch,-12,12); p->engine.intensity=std::clamp(intensity,0.0f,2.0f);
    p->engine.slowSpeed=std::clamp(slow,0.5f,0.95f);p->engine.fastSpeed=std::clamp(fast,1.05f,2.0f);
    if(p->engine.muted.exchange(muted!=0)!=(muted!=0)) p->engine.releaseEffects();
}
extern "C" void mnr_snapshot(Mnr* p,MnrSnapshot* s,char* error,uint32_t cap,int32_t meters) {
    auto& e=p->engine;
    *s={e.state.load(),e.muted.load(),e.stats.pitchActive.load(),e.stats.boostActive.load(),
        meters?e.stats.inputPeak.exchange(0):0,meters?e.stats.outputPeak.exchange(0):0,
        e.stats.processMs.load(),e.stats.outputQueue.load()/48.0f,e.stats.pitchDelayMs.load(),e.stats.pitchMaxMs.load(),
        e.stats.underruns.load(),e.stats.drops.load(),e.effectEpoch.load(),p->captured.exchange(0),
        e.stats.rvcState.load(),e.stats.rvcLatencyMs.load()};
    try {copy(mic::utf8(e.status()),error,cap);} catch(...) {copy("Status unavailable",error,cap);}
}
extern "C" int32_t mnr_devices(int32_t capture,char* result,uint32_t capacity) {
    try {
        std::string all;
        if(!capture) all="TAG\tThin Audio Gateway\n";
        for(auto& d:mic::devices(capture!=0)) {
            for(auto& ch:d.name) if(ch==L'\t'||ch==L'\r'||ch==L'\n') ch=L' ';
            all+=mic::utf8(d.id)+"\t"+mic::utf8(d.name)+"\n";
        }
        if(all.size()>=capacity) return 0;
        copy(all,result,capacity); return 1;
    } catch(const std::exception& e) {copy(e.what(),result,capacity);return 0;} catch(...) {return 0;}
}
extern "C" void mnr_bindings(Mnr* p,const uint32_t* keys,uint32_t count) {
    if(!keys || (count!=12 && count!=13))return;
    auto valid=[](unsigned k){return k==0 || ((k&255)>=3 && (k&255)<=254 && (k>>8)<=7);};
    for(unsigned i=0;i<count;++i){if(!valid(keys[i]))return;for(unsigned j=0;j<i;++j)if(keys[i]&&keys[i]==keys[j])return;}
    bool discord=false;for(unsigned i=0;i<13;++i){p->keys[i]=i<count?keys[i]:0;if(i>=5&&i<10&&keys[i])discord=true;}
    p->engine.desktopEnabled=discord;p->engine.releaseEffects();
}
extern "C" void mnr_alternate_intensity(Mnr* p,float intensity) {
    if(std::isfinite(intensity) && intensity>=0 && intensity<=2)p->engine.alternateIntensity=intensity;
}
extern "C" void mnr_capture_key(Mnr* p,int32_t enabled) {p->captured=0;p->capturing=enabled!=0;++p->captureGeneration;p->engine.releaseEffects();if(auto w=p->window.load())PostMessageW(w,WM_NULL,0,0);}
extern "C" void mnr_usage(uint64_t* cpu,uint64_t* memory) {
    FILETIME created{},ended{},kernel{},user{};PROCESS_MEMORY_COUNTERS counters{};counters.cb=sizeof(counters);
    GetProcessTimes(GetCurrentProcess(),&created,&ended,&kernel,&user);
    K32GetProcessMemoryInfo(GetCurrentProcess(),&counters,sizeof(counters));
    *cpu=((static_cast<uint64_t>(kernel.dwHighDateTime)<<32)|kernel.dwLowDateTime)+((static_cast<uint64_t>(user.dwHighDateTime)<<32)|user.dwLowDateTime);
    *memory=counters.WorkingSetSize;
}
extern "C" uint32_t mnr_events(Mnr* p) {return p->events.exchange(0);}
extern "C" void mnr_tray_hint(Mnr* p) {if(auto w=p->window.load()) PostMessageW(w,WM_APP+3,0,0);}
extern "C" int32_t mnr_replace_file(const char* from,uint32_t fl,const char* to,uint32_t tl) {
    try {return MoveFileExW(string(from,fl).c_str(),string(to,tl).c_str(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH)!=FALSE;} catch(...) {return 0;}
}
static void addTray(Mnr* p) {
    p->tray.uFlags=NIF_MESSAGE|NIF_ICON|NIF_TIP;
    if(!Shell_NotifyIconW(NIM_ADD,&p->tray)) p->events.fetch_or(16);
}
static HICON trayIcon() {
    auto icon=static_cast<HICON>(LoadImageW(GetModuleHandleW(nullptr),MAKEINTRESOURCEW(1),IMAGE_ICON,GetSystemMetrics(SM_CXSMICON),GetSystemMetrics(SM_CYSMICON),LR_SHARED));
    return icon?icon:LoadIconW(nullptr,IDI_APPLICATION);
}
static LRESULT CALLBACK shellProc(HWND w,UINT message,WPARAM wp,LPARAM lp) {
    auto p=reinterpret_cast<Mnr*>(GetWindowLongPtrW(w,GWLP_USERDATA));
    if(message==WM_NCCREATE) {p=static_cast<Mnr*>(reinterpret_cast<CREATESTRUCTW*>(lp)->lpCreateParams);SetWindowLongPtrW(w,GWLP_USERDATA,reinterpret_cast<LONG_PTR>(p));}
    if(!p) return DefWindowProcW(w,message,wp,lp);
    if(message==p->taskbar && p->taskbar) {addTray(p);return 0;}
    switch(message) {
    case WM_APP+4: EndMenu();return 0;
    case WM_APP+2: p->events.fetch_or(1);return 0;
    case WM_APP+3:
        p->tray.uFlags=NIF_INFO;
        wcscpy_s(p->tray.szInfoTitle,L"MicNoiseReducer работает в трее");
        wcscpy_s(p->tray.szInfo,L"Микрофон продолжает работать. Управление доступно в меню значка.");
        p->tray.dwInfoFlags=NIIF_INFO;Shell_NotifyIconW(NIM_MODIFY,&p->tray);return 0;
    case WM_APP+1:
        if(lp==WM_LBUTTONUP || lp==WM_LBUTTONDBLCLK) p->events.fetch_or(1);
        if(lp==WM_RBUTTONUP) {
            HMENU menu=CreatePopupMenu();
            AppendMenuW(menu,MF_STRING,1,L"Открыть MicNoiseReducer");
            AppendMenuW(menu,MF_STRING,4,L"Перезапустить");
            AppendMenuW(menu,MF_SEPARATOR,0,nullptr); AppendMenuW(menu,MF_STRING,2,L"Выход");
            POINT point;GetCursorPos(&point);SetForegroundWindow(w);
            const auto command=TrackPopupMenu(menu,TPM_RETURNCMD|TPM_RIGHTBUTTON,point.x,point.y,0,w,nullptr);
            DestroyMenu(menu);PostMessageW(w,WM_NULL,0,0);
            if(command) p->events.fetch_or(command);
        } return 0;
    case WM_WTSSESSION_CHANGE:
        if(wp==WTS_SESSION_LOCK || wp==WTS_CONSOLE_DISCONNECT || wp==WTS_REMOTE_DISCONNECT) p->locked=true;
        if(wp==WTS_SESSION_UNLOCK || wp==WTS_SESSION_LOGON || wp==WTS_REMOTE_CONNECT || wp==WTS_CONSOLE_CONNECT) p->locked=false;
        p->engine.releaseEffects();return 0;
    case WM_POWERBROADCAST:
        if(wp==PBT_APMSUSPEND) p->suspended=true;
        if(wp==PBT_APMRESUMEAUTOMATIC || wp==PBT_APMRESUMESUSPEND) p->suspended=false;
        p->engine.releaseEffects();return TRUE;
    case WM_QUERYENDSESSION: p->engine.releaseEffects();p->events.fetch_or(2);return TRUE;
    }
    return DefWindowProcW(w,message,wp,lp);
}
static bool down(unsigned key) {return (GetAsyncKeyState(static_cast<int>(key))&0x8000)!=0;}
static unsigned modifiers() {return (down(VK_CONTROL)?1:0)|(down(VK_MENU)?2:0)|(down(VK_SHIFT)?4:0);}
static bool bindable(unsigned k) {
    return k>=3 && k<=254 && k!=VK_CANCEL && k!=VK_SHIFT && k!=VK_CONTROL && k!=VK_MENU &&
        !(k>=VK_LSHIFT && k<=VK_RMENU) && k!=VK_LWIN && k!=VK_RWIN;
}
static bool desktopAvailable() {
    HDESK desktop=OpenInputDesktop(0,FALSE,DESKTOP_READOBJECTS);
    if(!desktop) return false;
    wchar_t name[128]{}; DWORD needed=0;
    const bool ok=GetUserObjectInformationW(desktop,UOI_NAME,name,sizeof(name),&needed) && _wcsicmp(name,L"Default")==0;
    CloseDesktop(desktop);return ok;
}
extern "C" int32_t mnr_shell_start(Mnr* p,char* error,uint32_t cap) {
    try {
        p->instance=CreateMutexW(nullptr,FALSE,L"Local\\MicNoiseReducer.SingleInstance");
        if(!p->instance) throw std::runtime_error("Cannot create instance mutex");
        if(GetLastError()==ERROR_ALREADY_EXISTS) {
            if(auto w=FindWindowW(L"MicNoiseReducer.RustShell",nullptr)) {AllowSetForegroundWindow(ASFW_ANY);PostMessageW(w,WM_APP+2,0,0);}
            else if(auto old=FindWindowW(L"MicNoiseReducer.Window",nullptr)) {ShowWindow(old,SW_SHOW);SetForegroundWindow(old);}
            return 0;
        }
        p->shell=std::jthread([p](std::stop_token stop) {
            try {mic::ensureTagHost();} catch(const std::exception& e) {p->engine.reportError(e.what());}
            WNDCLASSW cls{};cls.lpfnWndProc=shellProc;cls.hInstance=GetModuleHandleW(nullptr);cls.lpszClassName=L"MicNoiseReducer.RustShell";
            RegisterClassW(&cls);
            HWND w=CreateWindowExW(0,cls.lpszClassName,L"MicNoiseReducer background",0,0,0,0,0,nullptr,nullptr,cls.hInstance,p);
            if(!w) {p->events.fetch_or(16);return;}
            p->window=w;p->taskbar=RegisterWindowMessageW(L"TaskbarCreated");
            WTSRegisterSessionNotification(w,NOTIFY_FOR_THIS_SESSION);
            p->tray.cbSize=sizeof(p->tray);p->tray.hWnd=w;p->tray.uID=1;p->tray.uCallbackMessage=WM_APP+1;
            p->tray.hIcon=trayIcon();wcscpy_s(p->tray.szTip,L"MicNoiseReducer");addTray(p);
            mic::HoldLatch latch,replayLatch,noiseLatch; unsigned previousReplay=0; bool monitorArmed=false,captureArmed=false,capturedThisSession=false;unsigned captureGeneration=0;
            bool desktop=true;ULONGLONG lastDesktop=0,lastTick=GetTickCount64();
            while(!stop.stop_requested()) {
                MsgWaitForMultipleObjects(0,nullptr,FALSE,p->engine.running()||p->capturing?8:250,QS_ALLINPUT);
                MSG msg;while(PeekMessageW(&msg,nullptr,0,0,PM_REMOVE)) {TranslateMessage(&msg);DispatchMessageW(&msg);}
                const auto now=GetTickCount64();
                if(now-lastTick>250) p->engine.releaseEffects();
                lastTick=now;
                if(now-lastDesktop>=100) {desktop=desktopAvailable();lastDesktop=now;}
                const unsigned epoch=p->engine.effectEpoch;
                const bool capture=p->capturing;
                if(captureGeneration!=p->captureGeneration) {captureGeneration=p->captureGeneration;captureArmed=false;capturedThisSession=false;}
                if(capture && !capturedThisSession && desktop && !p->locked && !p->suspended) {
                    unsigned key=0;for(unsigned k=3;k<=254;++k) if(bindable(k)&&down(k)){key=k;break;}
                    if(!key) captureArmed=true;
                    else if(captureArmed) {p->captured=key==VK_ESCAPE?0xffffffffu:(key|(modifiers()<<8));capturedThisSession=true;}
                }
                const bool eligible=p->engine.running()&&p->engine.stats.outputActive&&!p->engine.muted&&!capture&&desktop&&!p->locked&&!p->suspended;
                unsigned keys[10]{};bool pressed[10]{};
                for(unsigned i=0;i<10;++i){keys[i]=p->keys[i];pressed[i]=keys[i]&&down(keys[i]&255);}
                const unsigned flags=latch.update(epoch,eligible,keys,pressed,modifiers(),down(VK_LWIN)||down(VK_RWIN),p->engine.stats.desktopState==2);
                unsigned replayKeys[]={p->keys[11]};bool replayPressed[]={replayKeys[0] && down(replayKeys[0]&255)};
                const auto replayFlags=replayLatch.update(epoch,eligible,replayKeys,replayPressed,modifiers(),down(VK_LWIN)||down(VK_RWIN));
                unsigned noiseKeys[]={p->keys[12]};bool noisePressed[]={noiseKeys[0] && down(noiseKeys[0]&255)};
                const auto noiseFlags=noiseLatch.update(epoch,eligible,noiseKeys,noisePressed,modifiers(),down(VK_LWIN)||down(VK_RWIN));
                if(replayFlags && !previousReplay && epoch==p->engine.effectEpoch)++p->engine.replayRequest;
                previousReplay=replayFlags;
                const unsigned monitorKey=p->keys[10];
                const bool monitorPressed=monitorKey&&down(monitorKey&255);
                const bool monitorEligible=p->engine.running()&&p->engine.stats.outputActive&&!capture&&desktop&&!p->locked&&!p->suspended;
                if(!monitorEligible) monitorArmed=false;
                else if(!monitorPressed) monitorArmed=true;
                else if(monitorArmed && modifiers()==(monitorKey>>8) && !down(VK_LWIN) && !down(VK_RWIN)) {p->events.fetch_or(8);monitorArmed=false;}
                // Do not publish after a concurrent reset; never carry a press into a new session.
                if(epoch==p->engine.effectEpoch) {
                    p->engine.heldSample=mic::packHeld(now,epoch,flags,eligible);
                    p->engine.noiseHeldSample=mic::packHeld(now,epoch,noiseFlags,eligible);
                }
            }
            p->engine.releaseEffects();Shell_NotifyIconW(NIM_DELETE,&p->tray);
            WTSUnRegisterSessionNotification(w);DestroyWindow(w);p->window=nullptr;
        });
        return 1;
    } catch(const std::exception& e) {copy(e.what(),error,cap);return -1;} catch(...) {copy("Shell initialization failed",error,cap);return -1;}
}
