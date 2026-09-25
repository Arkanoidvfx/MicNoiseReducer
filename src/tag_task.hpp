#pragma once
#include <string>
#include <filesystem>
#include <windows.h>

namespace mic {
std::wstring tagHostMode(const wchar_t* arguments);
std::filesystem::path tagHostPath();
bool tagMaintenancePending();
bool tagTaskEnabled(int mode=-1);
void stopTagHost();
// Caller verifies and locks the legacy image; requires maintenance and a closed UI.
void legacyTagHost(int operation);
bool tagHostFileCompatible(const std::filesystem::path& path) noexcept;
// -1: preserve login preference, 0/1: change it. Registration never stops audio.
bool configureTagTask(int login=-1);
bool tagTaskAutostart();
void runTagTask();
// A durable generation cancels stale starts and keeps the retry budget after both processes die.
GUID scheduledTagRecovery();
bool tagRecoveryCurrent(const GUID& generation);
bool takeTagRecovery(const GUID& generation);
void cancelTagRecovery(const GUID& generation={});
bool recoverTagTask();
enum class TagDeviceState : unsigned { Starting,WaitingDriver,WaitingEndpoint,Ready,Recovering,UserAction };
void tagRecoveryPhase(TagDeviceState phase,const GUID& generation);
TagDeviceState tagDeviceState(std::string& detail);
bool publishTagOwner(const GUID& generation,bool supervisor);
// Caller owns the returned handle. Never returns a different image/session/PID generation.
HANDLE tagHostProcess(DWORD access,bool managedOnly=false);
void removeTagTask();
std::string tagTaskWarning();
void setTagTaskWarning(const std::string& warning);
}
