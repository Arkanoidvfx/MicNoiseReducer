#pragma once
#include <rubberband/RubberBandLiveShifter.h>
#include <algorithm>
#include <cmath>
#include <vector>
#include <stdexcept>
#include <array>
#include <cstdint>
#include "hotkeys.hpp"

namespace mic {
// Fixed 10 ms transitions at 48 kHz. No lookahead in the gain stage.
struct Ramp {
    float value=0, target=0, step=0;
    unsigned left=0;
    explicit Ramp(float initial=0):value(initial),target(initial){}
    float next(float goal) {
        if(goal!=target) { target=goal; left=480; step=(target-value)/480; }
        if(left && !--left) value=target;
        else if(left) value+=step;
        return value;
    }
};
struct OutputEffects {
    Ramp gain{1},boost{3},wet{0},drive{0},discordGain{0.08f};
    void process(float* data,size_t n,float volume,float multiplier,bool held,bool overload=false,const uint8_t* discord=nullptr,float discordVolume=0.08f,uint8_t* modified=nullptr,const float* microphone=nullptr,float* effectOnly=nullptr) {
        const bool enabled=held && multiplier>1;
        for(size_t i=0;i<n;++i) {
            const float x=(std::isfinite(data[i])?data[i]:0)*gain.next(volume);
            const float b=boost.next(multiplier),mix=wet.next(enabled?1.0f:0.0f);
            if(modified && mix>0)modified[i]|=ModifiedBoost;
            const float harsh=drive.next(overload?1.0f:0.0f);
            const float sourceGain=discordGain.next(discordVolume);
            float effect=x;
            if(mix!=0){
                const float soft=0.891f*std::tanh(b*x/0.891f);
                const float distorted=std::lerp(soft,std::clamp(12*b*x,-0.891f,0.891f),harsh);
                effect=x+mix*(distorted-x);
            }
            data[i]=std::clamp(effect,-1.0f,1.0f)*(discord && discord[i]?sourceGain:1.0f);
            if(effectOnly)effectOnly[i]=data[i];
            if(microphone)data[i]=std::clamp(data[i]+microphone[i]*gain.value,-1.0f,1.0f);
        }
    }
};
struct SourceRouting {
    unsigned previous=0;
    bool phraseDiscord=false;
    unsigned phraseFlags=0;
    bool select(unsigned held,bool phraseActive) {
        const unsigned phrases=held&(HoldPhrases|(HoldPhrases<<DiscordShift));
        const bool conflict=(phrases&(phrases-1))!=0;
        if(phrases && !conflict && phrases!=previous)phraseDiscord=(phrases&(HoldPhrases<<DiscordShift))!=0;
        if(!phrases && !phraseActive)phraseDiscord=false;
        previous=phrases;
        phraseFlags=conflict?HoldPhrases:((held|(held>>DiscordShift))&HoldPhrases);
        return (phrases||phraseActive)?phraseDiscord:(held&(HoldLive<<DiscordShift))!=0;
    }
};
// One bounded, in-memory slot shared by all hold effects (20 s covers 10 s at x0.5).
class LastEffect {
    std::vector<float> audio_=std::vector<float>(48000*20);
    std::vector<uint8_t> modified_=std::vector<uint8_t>(48000*20);
    size_t count_=0,position_=0;
    unsigned epoch_=0,cancel_=0,request_=0,previous_=0;
    bool capturing_=false,playing_=false,discord_=false;
public:
    bool process(float* data,size_t n,uint8_t* modified,bool& discord,
                 unsigned held,bool phraseActive,bool valid,unsigned epoch,unsigned cancel,unsigned request) {
        if(epoch!=epoch_ || cancel!=cancel_ || !valid){
            if(capturing_)count_=0;
            capturing_=playing_=false;previous_=0;epoch_=epoch;cancel_=cancel;request_=request;
            return false;
        }
        const bool active=held || phraseActive;
        if(held && held!=previous_){count_=0;capturing_=true;playing_=false;discord_=discord;}
        previous_=held;
        if(capturing_){
            for(size_t i=0;i<n;++i)if(modified[i] && count_<audio_.size()){
                modified_[count_]=modified[i];audio_[count_++]=data[i];
            }
            if(!active)capturing_=false;
        }
        if(request!=request_){
            request_=request;
            if(!active && !capturing_ && count_){position_=0;playing_=true;}
        }
        if(!playing_)return false;
        discord=discord_;
        for(size_t i=0;i<n;++i){
            // The live microphone accompanies replay, but is never saved into a Discord clip.
            if(position_<count_){modified[i]=modified_[position_];data[i]=audio_[position_++];}
            else {data[i]=0;modified[i]=0;}
        }
        playing_=position_<count_;
        return true;
    }
};
// A bounded phrase recorder: tape speed or live speech followed by delayed reverse.
// 32-tap windowed sinc prevents aliasing on acceleration; the live bypass is exact.
class PhraseEffect {
public:
    // Numeric values are the C ABI contract (mnr_phrase_state) and the Rust UI's match arms.
    enum State : int {
        Idle=0, RecordSlow=1, RecordFast=2, PlaySlow=3, PlayFast=4, TailSlow=5, TailFast=6,
        FullSlow=7, FullFast=8, RecordReverse=9, ReversePause=10, FullReverse=11, PlayReverse=12
    };
private:
    static constexpr size_t limit=48000*10, phases=256, taps=32;
    std::vector<float> audio_=std::vector<float>(limit);
    std::array<std::array<float,taps>,phases> filter_{};
    size_t count_=0,tail_=0,played_=0;
    std::array<float,limit/480> reversePeaks_{};
    size_t reverseBegin_=0,reverseEnd_=0;
    double position_=0,speed_=1;
    unsigned mode_=0,lastHeld_=0,epoch_=0,cancel_=0;
    bool blocked_=false;
    int state_=Idle;
    Ramp live_{1};
    bool reverse() const{return mode_==HoldReverse;}
    bool playing() const{return state_==PlaySlow||state_==PlayFast||state_==PlayReverse;}
    void clear(){count_=tail_=played_=reverseBegin_=reverseEnd_=0;position_=0;mode_=0;state_=Idle;}
    void trimReverse() {
        reverseBegin_=0;reverseEnd_=count_;
        if(count_<14400)return;
        const size_t windows=(count_+479)/480;
        const float peak=*std::max_element(reversePeaks_.begin(),reversePeaks_.begin()+windows);
        if(peak<=0.001f)return;
        // ponytail: only near-silence; noisy edges intentionally stay intact.
        const float threshold=std::min(0.0001f,peak*0.001f);
        size_t first=0,last=windows;
        while(first<windows && reversePeaks_[first]<threshold)++first;
        while(last>first && reversePeaks_[last-1]<threshold)--last;
        if(first==last)return;
        if(first*480>=9600)reverseBegin_=first*480-4800;
        if(last*480<=count_ && count_-last*480>=9600)reverseEnd_=last*480+4800;
        if(reverseBegin_>=reverseEnd_ || reverseEnd_>count_){reverseBegin_=0;reverseEnd_=count_;}
    }
    size_t playbackSize() const{return reverse()?reverseEnd_-reverseBegin_:count_;}
    void begin(unsigned mode,float speed) {
        clear();mode_=mode;speed_=speed;state_=mode==HoldReverse?RecordReverse:(mode==HoldSlow?RecordSlow:RecordFast);
        if(reverse())return; // Reverse uses exact recorded samples at normal speed.
        const double cutoff=std::min(1.0,1.0/speed_)*0.94;
        for(size_t p=0;p<phases;++p){
            double sum=0;
            for(size_t j=0;j<taps;++j){
                const double x=static_cast<double>(j)-15-static_cast<double>(p)/phases;
                const double a=3.141592653589793*x*cutoff;
                const double v=cutoff*(std::abs(a)<1e-9?1:std::sin(a)/a)*(0.5+0.5*std::cos(3.141592653589793*x/16));
                filter_[p][j]=static_cast<float>(v);sum+=v;
            }
            for(auto& v:filter_[p])v=static_cast<float>(v/sum);
        }
    }
    bool recording() const{return state_!=Idle && !playing() && state_!=ReversePause;}
public:
    int state() const{return state_;}
    float seconds() const{return static_cast<float>(state_==ReversePause?tail_:(playing()?(playbackSize()-position_)/speed_:count_))/48000;}
    void process(float* data,size_t n,unsigned held,float slow,float fast,bool valid,unsigned epoch,unsigned cancel,bool reverseLive=true,uint8_t* modified=nullptr) {
        const unsigned mode=held&HoldPhrases;
        const bool conflict=(mode&(mode-1))!=0;
        if(epoch!=epoch_ || cancel!=cancel_ || !valid || conflict){
            clear();blocked_=mode!=0;epoch_=epoch;cancel_=cancel;
        }
        if(!mode)blocked_=false;
        if(valid && !blocked_ && mode && mode!=lastHeld_ && !conflict)
            begin(mode,mode==HoldReverse?1.0f:std::clamp(mode==HoldSlow?slow:fast,mode==HoldSlow?0.5f:1.05f,mode==HoldSlow?0.95f:2.0f));
        if(valid && recording() && !mode && lastHeld_){
            if(reverse())trimReverse();
            tail_=reverse()?7200:std::min<size_t>(9600,limit-count_);
            state_=reverse()?ReversePause:(mode_==HoldSlow?TailSlow:TailFast);
        }
        lastHeld_=mode;
        if(state_==Idle && live_.value==1)return; // No sample work or added delay in the normal live path.
        for(size_t i=0;i<n;++i){
            const float dry=std::isfinite(data[i])?data[i]:0;
            if(state_==ReversePause){
                if(modified)modified[i]=0;
                data[i]=0;live_.next(0);
                if(!--tail_){state_=PlayReverse;position_=0;played_=0;if(!playbackSize())clear();}
                continue;
            }
            if(recording()) {
                if(count_<limit){
                    if(reverse()){
                        auto& peak=reversePeaks_[count_/480];
                        if(count_%480==0)peak=0;
                        peak=std::max(peak,std::abs(dry));
                    }
                    audio_[count_++]=dry;
                }
                if(state_==TailSlow||state_==TailFast){
                    if(tail_)--tail_;
                    if(!tail_){state_=mode_==HoldSlow?PlaySlow:PlayFast;position_=0;played_=0;}
                }else if(count_==limit)state_=reverse()?FullReverse:(mode_==HoldSlow?FullSlow:FullFast);
                const float live=live_.next(reverseLive?1.0f:0.0f);
                if(modified)modified[i]=0;
                data[i]=reverseLive?dry*live:0;continue;
            }
            if(playing()){
                if(modified)modified[i]=ModifiedEffects;
                const auto center=static_cast<int64_t>(position_);
                const auto phase=std::min(phases-1,static_cast<size_t>((position_-center)*phases));
                double v=0;
                const size_t length=playbackSize();
                if(reverse()) v=audio_[reverseEnd_-1-played_];
                else for(size_t j=0;j<taps;++j){const auto at=center+static_cast<int64_t>(j)-15;if(at>=0&&static_cast<size_t>(at)<count_)v+=audio_[static_cast<size_t>(at)]*filter_[phase][j];}
                double fade=std::min({1.0,played_/480.0,(count_-position_)/(speed_*480)});
                if(reverse()){
                    // Ease the reverse ending to exact silence over 60 ms (half of a very short phrase).
                    const double end=std::clamp((length-position_-1)/std::min(2880.0,length/2.0),0.0,1.0);
                    fade=std::min(played_/480.0,end*end*(3-2*end));
                }
                data[i]=std::clamp(static_cast<float>(v*fade),-1.0f,1.0f);
                position_+=speed_;++played_;
                if(position_>=length)clear();
            }else data[i]=dry*live_.next(1);
        }
    }
};
class PitchEffect {
    RubberBand::RubberBandLiveShifter shifter_{48000,1,0};
    const size_t size_=shifter_.getBlockSize();
    std::vector<float> input_=std::vector<float>(size_),output_=std::vector<float>(size_);
    size_t in_=0,out_=0,total_=0,delay_=0;
    bool hasOutput_=false,started_=false;
    int semitones_=0;
    Ramp wet_;
    void begin(int semitones) {
        shifter_.reset(); shifter_.setPitchScale(std::exp2(semitones/12.0));
        delay_=shifter_.getStartDelay();
        in_=out_=total_=0; hasOutput_=false; started_=true; semitones_=semitones;
    }
public:
    void reset() { shifter_.reset();in_=out_=total_=delay_=0;hasOutput_=started_=false;wet_=Ramp{}; }
    bool active() const {return wet_.value>0;}
    float delayMs() const {return static_cast<float>(delay_+size_)*1000/48000;}
    void process(float* data,size_t n,int semitones,bool held,uint8_t* modified=nullptr) {
        const bool wanted=held && semitones!=0;
        for(size_t i=0;i<n;++i) {
            if(started_ && wet_.value==0 && (!wanted || semitones!=semitones_)) started_=false;
            if(!started_ && wanted) begin(semitones);
            if(!started_) continue; // Exact dry bypass, no pitch work or buffering.
            const float dry=data[i];
            const bool valid=hasOutput_ && total_>=delay_+size_;
            const float shifted=hasOutput_?output_[out_++]:0;
            if(out_==size_) hasOutput_=false;
            input_[in_++]=dry; ++total_;
            if(in_==size_) {
                const float* source=input_.data(); float* destination=output_.data();
                shifter_.shift(&source,&destination);
                in_=out_=0; hasOutput_=true;
            }
            const float mix=wet_.next(wanted && semitones==semitones_ && valid?1.0f:0.0f);
            if(modified && mix>0)modified[i]=ModifiedEffects;
            data[i]=mix==0?dry:dry+mix*(shifted-dry);
            if(!std::isfinite(data[i])) throw std::runtime_error("Pitch returned non-finite audio");
            data[i]=std::clamp(data[i],-1.0f,1.0f);
        }
    }
};
}
