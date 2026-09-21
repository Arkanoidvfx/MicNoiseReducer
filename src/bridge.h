#pragma once
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct Mnr Mnr;
typedef struct {
    int32_t state,muted,pitch_active,boost_active;
    float input_peak,output_peak,process_ms,queue_ms,pitch_delay_ms,pitch_max_ms;
    uint32_t underruns,drops,epoch,captured_key;
    int32_t rvc_state;
    float rvc_latency_ms;
} MnrSnapshot;
Mnr* mnr_create(char* error,uint32_t capacity);
void mnr_destroy(Mnr*);
int32_t mnr_start(Mnr*,const char* input,uint32_t input_len,const char* output,uint32_t output_len,
    int32_t version,uint32_t buffer,uint32_t period,int32_t graphs,float intensity,char* error,uint32_t capacity);
void mnr_stop(Mnr*);
int32_t mnr_headphones(Mnr*,int32_t enabled,const char* output,uint32_t length,int32_t denoise,char* error,uint32_t capacity);
void mnr_headphone_controls(Mnr*,float intensity,float volume,int32_t pitch,int32_t muted);
int32_t mnr_headphone_state(Mnr*,char* text,uint32_t capacity);
// mode: 0 off, 1 entire virtual microphone, 2 modified samples only.
// 0 off, 1 full voice, 2 other effects, 3 boost, 4 both effect groups.
int32_t mnr_monitor(Mnr*,int32_t mode,char* error,uint32_t capacity);
int32_t mnr_monitor_state(Mnr*,char* text,uint32_t capacity);
void mnr_controls(Mnr*,float volume,float boost,int32_t pitch,float intensity,int32_t muted,float slow,float fast,int32_t overload,float discordVolume,int32_t rvcEnabled);
void mnr_rvc_settings(Mnr*,uint32_t slot,int32_t pitch,uint32_t index,uint32_t chunk_ms,uint32_t gain);
int32_t mnr_phrase_state(Mnr*,float* seconds);
int32_t mnr_discord_state(Mnr*,char* text,uint32_t capacity,int32_t* active);
void mnr_phrase_cancel(Mnr*);
void mnr_snapshot(Mnr*,MnrSnapshot*,char* error,uint32_t capacity,int32_t meters);
int32_t mnr_devices(int32_t capture,char* result,uint32_t capacity);
void mnr_bindings(Mnr*,const uint32_t* keys,uint32_t count);
void mnr_alternate_intensity(Mnr*,float intensity);
void mnr_capture_key(Mnr*,int32_t enabled);
uint32_t mnr_events(Mnr*);
int32_t mnr_shell_start(Mnr*,char* error,uint32_t capacity);
void mnr_tray_hint(Mnr*);
int32_t mnr_replace_file(const char* from,uint32_t from_len,const char* to,uint32_t to_len);
void mnr_usage(uint64_t* cpu_100ns,uint64_t* working_set);
#ifdef __cplusplus
}
#endif
