#pragma once
#include <windows.h>
#include <array>
#include <atomic>
#include <algorithm>
#include <cstdint>
#include <filesystem>
#include <mutex>
#include <string>
#include <thread>
#include <vector>
#include "hotkeys.hpp"

namespace mic {
constexpr unsigned rate = 48000, block = 480;
struct RvcSettings {
    unsigned slot=0, chunkMs=200, index=0, gain=100;
    int pitch=0;
    uint64_t packed() const {return slot | (uint64_t(pitch+24)<<16) | (uint64_t(index)<<22) | (uint64_t(chunkMs)<<29) | (uint64_t(gain)<<39);}
    static RvcSettings unpack(uint64_t v) {return {unsigned(v&65535),unsigned((v>>29)&1023),unsigned((v>>22)&127),unsigned((v>>39)&511),int((v>>16)&63)-24};}
};
struct Device { std::wstring id, name; };
inline int preferredDevice(const std::vector<Device>& list,const std::wstring& saved,const std::wstring& hint) {
    for(size_t i=0;i<list.size();++i)
        if(saved.empty()?list[i].name.find(hint)!=std::wstring::npos:list[i].id==saved) return static_cast<int>(i);
    return -1; // A missing saved device must never silently select a different microphone.
}
std::vector<Device> devices(bool capture);
std::filesystem::path projectRoot();
std::string utf8(const std::wstring& s);
std::wstring wide(const std::string& s);

// Single producer / single consumer. Only the consumer may discard old samples.
template<size_t Capacity,class T=float> class Ring {
    std::array<T, Capacity> data_{};
    alignas(64) std::atomic<uint64_t> head_{0};
    alignas(64) std::atomic<uint64_t> tail_{0};
public:
    size_t size() const {
        const auto t = tail_.load(std::memory_order_acquire);
        return static_cast<size_t>(head_.load(std::memory_order_acquire) - t);
    }
    bool push(const T* p, size_t n) {
        const auto h = head_.load(std::memory_order_relaxed);
        if(n > Capacity || h - tail_.load(std::memory_order_acquire) + n > Capacity) return false;
        for(size_t i = 0; i < n; ++i) data_[(h+i)%Capacity] = p[i];
        head_.store(h+n, std::memory_order_release);
        return true;
    }
    bool pop(T* p, size_t n) {
        const auto t = tail_.load(std::memory_order_relaxed);
        if(head_.load(std::memory_order_acquire) - t < n) return false;
        for(size_t i = 0; i < n; ++i) p[i] = data_[(t+i)%Capacity];
        tail_.store(t+n, std::memory_order_release);
        return true;
    }
    size_t trim(size_t keep) {
        const auto t = tail_.load(std::memory_order_relaxed);
        const auto available = head_.load(std::memory_order_acquire) - t;
        const auto drop = available > keep ? available - keep : 0;
        tail_.store(t+drop, std::memory_order_release);
        return static_cast<size_t>(drop);
    }
    void reset() { head_=0; tail_=0; } // Only after both threads have joined.
};

struct Drift {
    double filtered = 0, integral = 0;
    double update(double errorFrames) {
        filtered += 0.05 * (errorFrames-filtered);
        integral = std::clamp(integral + filtered*0.00000001, -0.0015, 0.0015);
        return std::clamp(integral + filtered*0.000003, -0.003, 0.003);
    }
};
struct Config {
    std::wstring input, output;
    int version = 1;
    float intensity = 1;
    unsigned bufferMs = 40;
    unsigned periodMs = 5;
    std::filesystem::path sdk;
    bool tag = false;
    std::filesystem::path tagSdk;
    int cudaGraphs = -1; // -1: SDK default, 0: disabled, 1: enabled.
};
void benchmarkAfx(const Config&, const std::vector<float>&, unsigned seconds, const std::filesystem::path& csv);
struct TagClock {
    double pending = 0;
    unsigned take(double elapsed, double correction, unsigned maximum) {
        pending += elapsed*rate*(1+correction);
        const auto frames=static_cast<unsigned>(std::min(pending,static_cast<double>(maximum)));
        pending -= frames;
        return frames;
    }
};
struct Stats {
    std::atomic<int> desktopState{0}; // Off, Starting, Ready, Error
    std::atomic<bool> desktopSource{false};
    std::atomic<int> phraseState{0};
    std::atomic<float> phraseSeconds{0};
    std::atomic<bool> pitchActive{false}, boostActive{false};
    std::atomic<float> pitchDelayMs{0}, pitchMaxMs{0};
    std::atomic<bool> outputActive{false};
    std::atomic<float> reconfigureMs{0};
    std::atomic<float> maxRunMs{0}, maxResetMs{0};
    std::atomic<float> inputPeak{0}, outputPeak{0}, processMs{0}, maxProcessMs{0};
    std::atomic<int> rvcState{0}; // Off, Starting, Ready, Bypass
    std::atomic<float> rvcLatencyMs{0};
    std::atomic<unsigned> inputQueue{0}, outputQueue{0}, renderPadding{0};
    std::atomic<unsigned> underruns{0}, drops{0}, discontinuities{0}, processed{0};
    std::atomic<float> inputPeriodMs{0}, outputPeriodMs{0}, driftPpm{0};
    std::atomic<unsigned> tagBufferFrames{0}, tagDriverGaps{0};
    std::atomic<uint64_t> tagFrames{0};
    std::atomic<unsigned> tagLateTicks{0}, tagReconnects{0};
    std::atomic<float> tagMaxWakeMs{0};
};
struct RoutedSample {float value=0;uint8_t discord=0,modified=0;unsigned epoch=0;float microphone=0;};
// RVC audio crosses the worker boundary tagged with a generation: both rings are aligned
// streams of the same sample index space, so a new generation starts at index 0 on both sides.
struct RvcSample {float sample=0;unsigned generation=0;};
// ponytail: fixed inference/scheduling budget; raise if rvcState 3 shows up in normal use.
constexpr unsigned rvcSlack=9600; // 200 ms
// Fixed-delay playout: input index j is heard at j+delay. Late audio is dropped, never replayed;
// missing audio is silence. Latency therefore never creeps and speech is never heard twice.
struct RvcPlayout {
    uint64_t pushed=0,consumed=0;
    unsigned generation=0;
    // Returns 1 while priming, 2 when converted audio was output, 3 on underrun (silence).
    template<class Ring> int process(Ring& ring,float* out,unsigned count,uint8_t* modified,unsigned delay,unsigned current) {
        if(current!=generation){generation=current;pushed=consumed=0;}
        const int64_t first=static_cast<int64_t>(pushed)-delay;
        pushed+=count;
        std::fill_n(out,count,0.0f);
        if(modified)std::fill_n(modified,count,static_cast<uint8_t>(ModifiedEffects)); // the whole RVC region is an effect; silence previews as 0
        if(first+static_cast<int64_t>(count)<=0)return 1;
        RvcSample s;
        auto next=[&]{while(ring.pop(&s,1))if(s.generation==current)return true;return false;};
        if(first>0){
            while(consumed<static_cast<uint64_t>(first) && next())++consumed;
            if(consumed<static_cast<uint64_t>(first))return 3;
        }
        for(unsigned i=first<0?static_cast<unsigned>(-first):0;i<count;++i){
            if(!next())return 3;
            out[i]=s.sample;++consumed;
        }
        return 2;
    }
};
inline float previewSample(const RoutedSample& sample,uint8_t mask,unsigned epoch,bool audible) {
    return audible && (sample.modified&mask) && sample.epoch==epoch?sample.value:0;
}
class Engine {
    friend void checkDiscordCapture(unsigned seconds);
    friend class Monitor;
    HANDLE stop_ = nullptr, data_ = nullptr, ready_ = nullptr;
    HANDLE tagOwner_ = nullptr;
    Config config_;
    std::thread io_, dsp_, desktopThread_;
    Ring<16384> captured_, desktop_;
    Ring<16384,RoutedSample> cleaned_;
    Ring<16384,RoutedSample> preview_;
    std::atomic<uint8_t> previewMask_{0};
    void preview(const float* audio,const RoutedSample* routed,const uint8_t* modified,unsigned count);
    std::atomic<bool> resetEffect_{false}, running_{false};
    mutable std::mutex statusMutex_;
    std::wstring status_ = L"Stopped";
    std::wstring desktopMessage_;
    void ioLoop(Config config);
    void tagLoop(Config config);
    void dspLoop(Config config);
    void desktopLoop();
    void fail(const std::exception& error);
    void status(std::wstring text);
public:
    Stats stats;
    // One atomic message: timestamp (37 bits), epoch (16), eligibility + ten holds (11).
    std::atomic<uint64_t> heldSample{0};
    std::atomic<uint64_t> noiseHeldSample{0};
    std::atomic<unsigned> effectEpoch{0};
    std::atomic<bool> desktopEnabled{false};
    std::wstring desktopMessage() const;
    std::atomic<int> state{0}; // Stopped, Loading, WaitingClient, Running, Stopping, Error
    std::atomic<float> volume{1}, boost{3};
    std::atomic<bool> overload{false};
    std::atomic<float> discordVolume{0.08f};
    std::atomic<float> slowSpeed{0.7f},fastSpeed{1.5f};
    std::atomic<unsigned> phraseCancel{0};
    std::atomic<unsigned> replayRequest{0};
    std::atomic<int> pitch{-5};
    unsigned held() const {
        const auto sample=heldSample.load();
        return heldFlags(sample,effectEpoch.load(),GetTickCount64(),running_ && stats.outputActive && !muted);
    }
    void releaseEffects() { heldSample=0; noiseHeldSample=0; ++effectEpoch; }
    void reportError(const std::string& message);
    std::atomic<float> intensity{1};
    std::atomic<float> alternateIntensity{0.15f};
    std::atomic<bool> rvcEnabled{false};
    std::atomic<uint64_t> rvcConfig{RvcSettings{}.packed()};
    std::atomic<bool> muted{false};
    // Test hook: the TAG output thread sleeps once for this many ms (simulated preemption).
    std::atomic<unsigned> testStallMs{0};
    Engine();
    ~Engine();
    Engine(const Engine&) = delete;
    Engine& operator=(const Engine&) = delete;
    void start(const Config& config);
    void stop();
    bool running() const { return running_; }
    std::wstring status() const;
};
void checkDiscordCapture(unsigned seconds);
void checkRvc();
void checkRvcIdle();
struct StereoSample {float left=0,right=0; unsigned epoch=0;};
void checkHeadphones(const std::wstring& output,bool denoise);
class Headphones {
    HANDLE stop_=nullptr,data_=nullptr,ready_=nullptr;
    HANDLE owner_=nullptr;
    std::thread io_,dsp_;
    Ring<16384,StereoSample> captured_,cleaned_;
    std::atomic<unsigned> epoch_{0};
    mutable std::mutex mutex_;
    std::wstring message_;
    void fail(const std::exception&);
    void ioLoop(std::wstring output);
    void dspLoop(bool denoise);
public:
    std::atomic<int> state{0},pitch{0}; // Off, Loading, Ready, Error
    std::atomic<float> intensity{0.8f},volume{0.7f};
    std::atomic<bool> muted{false};
    std::atomic<unsigned> processed{0},drops{0};
    Headphones();
    ~Headphones();
    void start(const std::wstring& output,bool denoise);
    void stop();
    std::wstring message() const;
};
// Independent preview of the virtual microphone. It never waits in the DSP/output path.
class Monitor {
    Engine& engine_;
    HANDLE stop_=nullptr;
    std::thread thread_;
    mutable std::mutex mutex_;
    std::wstring message_;
public:
    std::atomic<int> state{0}; // Off, Starting, Listening, Error
    std::atomic<uint64_t> frames{0};
    std::atomic<float> renderedPeak{0};
    explicit Monitor(Engine& engine);
    ~Monitor();
    void start(const std::wstring& route,uint8_t effectsMask=0);
    void stop();
    std::wstring message() const;
};
}
