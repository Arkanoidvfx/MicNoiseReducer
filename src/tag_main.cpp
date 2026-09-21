#include "audio.hpp"
#include <iostream>
#include <chrono>
#include <stdexcept>
static std::atomic<bool> done=false;
static BOOL WINAPI control(DWORD type) {if(type==CTRL_C_EVENT || type==CTRL_BREAK_EVENT){done=true;return TRUE;}return FALSE;}
int main(int argc,char** argv) {
    try {
        if(argc<2 || argc>6){std::cerr<<"Usage: mic_tag INPUT_INDEX [SECONDS=0 until Ctrl+C] [RESERVE_MS=40] [STOP_FILE] [CUDA_GRAPHS=-1; SDK default]\n";return 2;}
        const int input=std::stoi(argv[1]), seconds=argc>2?std::stoi(argv[2]):0, reserve=argc>3?std::stoi(argv[3]):40;
        const std::filesystem::path stopFile=argc>4 && std::string(argv[4])!="-"?mic::wide(argv[4]):L"";
        if(!stopFile.empty() && std::filesystem::exists(stopFile)) throw std::runtime_error("Stop file already exists");
        auto devices=mic::devices(true);
        if(input<0 || static_cast<size_t>(input)>=devices.size() || seconds<0 || seconds>86400 || reserve<10 || reserve>80)
            throw std::runtime_error("Invalid input/duration/reserve: input="+std::to_string(input)+" devices="+std::to_string(devices.size())+" seconds="+std::to_string(seconds)+" reserve="+std::to_string(reserve));
        SetConsoleCtrlHandler(control,TRUE);
        mic::Config c; c.input=devices[input].id; c.output=L"TAG"; c.version=2; c.bufferMs=reserve;
        c.cudaGraphs=argc>5?std::stoi(argv[5]):-1;
        c.sdk=mic::projectRoot()/L"vendor/nvidia-afx-3.0.0";
        c.tag=true; c.tagSdk=mic::projectRoot()/L"vendor/tag-2.0.0.1903-demo";
        mic::Engine engine; engine.start(c);
        std::cout<<"Input: "<<mic::utf8(devices[input].name)<<"; NVIDIA v2 -> TAG; reserve="<<reserve<<" ms; CUDA graphs="<<c.cudaGraphs<<"\n"<<std::flush;
        unsigned elapsed=0;
        while(!done && (!seconds || elapsed<static_cast<unsigned>(seconds))) {
            std::this_thread::sleep_for(std::chrono::seconds(1)); ++elapsed;
            if(!stopFile.empty() && std::filesystem::exists(stopFile)) break;
            if(!engine.running()) throw std::runtime_error(mic::utf8(engine.status()));
            if(elapsed==1 || elapsed%5==0) {
                auto& s=engine.stats;
                std::cout<<mic::utf8(engine.status())<<" blocks="<<s.processed<<" queue_ms="<<s.outputQueue/48.0
                    <<" last_ms="<<s.processMs<<" max_ms="<<s.maxProcessMs<<" gaps="<<s.underruns<<" drops="<<s.drops
                    <<" run_max_ms="<<s.maxRunMs<<" reset_max_ms="<<s.maxResetMs
                    <<" TAG_buffer="<<s.tagBufferFrames<<" TAG_frames="<<s.tagFrames<<" TAG_gaps="<<s.tagDriverGaps
                    <<" late_ticks="<<s.tagLateTicks<<" max_wake_ms="<<s.tagMaxWakeMs
                    <<" drift_ppm="<<s.driftPpm<<"\n"<<std::flush;
            }
        }
        engine.stop(); std::cout<<"Stopped\n";
        return engine.stats.underruns || engine.stats.drops || engine.stats.tagDriverGaps ? 1:0;
    } catch(const std::exception& e){std::cerr<<"ERROR: "<<e.what()<<"\n";return 1;}
}
