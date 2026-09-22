#pragma once
#include <cstdint>
#include <cstddef>
namespace mic {
// Hold bit layout shared by the shell latch, DSP routing and the checks: five microphone
// effects in bits 0-4, the same five for Discord shifted by DiscordShift.
enum Hold : unsigned {
    HoldBoost=1, HoldPitch=2, HoldSlow=4, HoldFast=8, HoldReverse=16,
    HoldPhrases=HoldSlow|HoldFast|HoldReverse, HoldLive=HoldBoost|HoldPitch,
    DiscordShift=5, HoldMicMask=31, HoldAllMask=1023
};
// Per-sample effect categories carried through the output queues for effects-only monitoring.
enum Modified : uint8_t { ModifiedEffects=1, ModifiedBoost=2, ModifiedSound=4 }; // ModifiedSound: monitor mask bit; set only on preview-queue samples
inline uint64_t packHeld(uint64_t now,unsigned epoch,unsigned flags,bool eligible=true) {
    return (now<<27)|((static_cast<uint64_t>(epoch)&65535)<<11)|(eligible?1024:0)|(flags&HoldAllMask);
}
inline bool heldFresh(uint64_t sample,unsigned epoch,uint64_t now) {
    return (sample&1024) && ((sample>>11)&65535)==(epoch&65535) && now-(sample>>27)<=250;
}
inline unsigned heldFlags(uint64_t sample,unsigned epoch,uint64_t now,bool eligible) {
    return eligible && heldFresh(sample,epoch,now)?static_cast<unsigned>(sample&HoldAllMask):0;
}
inline float heldIntensity(float normal,float alternate,uint64_t sample,unsigned epoch,uint64_t now,bool eligible) {
    return (heldFlags(sample,epoch,now,eligible)&1)?alternate:normal;
}
struct HoldLatch {
    bool armed[10]{};
    unsigned epoch=0;
    template<size_t N> unsigned update(unsigned currentEpoch,bool eligible,const unsigned (&keys)[N],const bool (&pressed)[N],unsigned mods,bool win,bool discordReady=true) {
        static_assert(N<=10);
        if(epoch!=currentEpoch){epoch=currentEpoch;for(auto& a:armed)a=false;}
        unsigned flags=0;
        for(unsigned i=0;i<N;++i) {
            const bool allowed=eligible && (i<5 || discordReady);
            if(!allowed) armed[i]=false;
            else if(!pressed[i]) armed[i]=true;
            if(allowed && armed[i] && keys[i] && pressed[i] && mods==(keys[i]>>8) && !win) flags|=1<<i;
        }
        return flags;
    }
};
}
