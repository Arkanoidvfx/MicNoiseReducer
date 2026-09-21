#include "audio.hpp"
#include <commctrl.h>
#include <shellapi.h>
#include <cmath>
#include <memory>
#include <sstream>
#include <iomanip>

#pragma comment(linker,"/manifestdependency:\"type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'\"")

namespace {
constexpr UINT trayMessage=WM_APP+1;
enum {Input=100,Output,Version,Intensity,Buffer,Start,Mute,Refresh,Tray,Show,Exit};
const COLORREF background=RGB(242,245,243), ink=RGB(23,43,38), mutedInk=RGB(91,110,102), accent=RGB(0,112,85);
HWND window=nullptr, inputBox=nullptr,outputBox=nullptr,versionBox=nullptr,intensitySlider=nullptr,bufferBox=nullptr,startButton=nullptr,muteButton=nullptr,statusLabel=nullptr,detailBox=nullptr;
HFONT font=nullptr,titleFont=nullptr;
HBRUSH backgroundBrush=nullptr,whiteBrush=nullptr;
UINT dpi=96;
std::unique_ptr<mic::Engine> engine;
std::vector<mic::Device> inputs,outputs;
std::filesystem::path root,settings;
bool trayAdded=false,wasRunning=false;
NOTIFYICONDATAW tray{};
std::wstring lastStatus,lastDetail;
const UINT taskbarCreated=RegisterWindowMessageW(L"TaskbarCreated");
int px(int n) { return MulDiv(n,static_cast<int>(dpi),96); }
void text(HDC dc,int x,int y,int w,int h,const std::wstring& s,bool title=false,COLORREF color=ink) {
    SelectObject(dc,title?titleFont:font); SetTextColor(dc,color); SetBkMode(dc,TRANSPARENT);
    RECT r{px(x),px(y),px(x+w),px(y+h)};
    DrawTextW(dc,s.c_str(),-1,&r,DT_LEFT|DT_TOP|DT_WORDBREAK|DT_NOPREFIX);
}
HWND control(const wchar_t* type,const wchar_t* label,DWORD style,int x,int y,int w,int h,int id) {
    HWND result=CreateWindowExW(0,type,label,WS_CHILD|WS_VISIBLE|style,px(x),px(y),px(w),px(h),window,reinterpret_cast<HMENU>(static_cast<INT_PTR>(id)),GetModuleHandleW(nullptr),nullptr);
    if(!result) throw std::runtime_error("Cannot create interface control");
    SendMessageW(result,WM_SETFONT,reinterpret_cast<WPARAM>(font),TRUE); return result;
}
std::wstring setting(const wchar_t* key,const wchar_t* fallback=L"") {
    wchar_t value[2048]; GetPrivateProfileStringW(L"audio",key,fallback,value,2048,settings.c_str()); return value;
}
void save(const wchar_t* key,const std::wstring& value) {
    if(!WritePrivateProfileStringW(L"audio",key,value.c_str(),settings.c_str())) throw std::runtime_error("Cannot save settings.ini");
}
int selection(HWND h) { return static_cast<int>(SendMessageW(h,CB_GETCURSEL,0,0)); }
void refill(HWND box,const std::vector<mic::Device>& list,const std::wstring& preferred,const std::wstring& hint) {
    SendMessageW(box,CB_RESETCONTENT,0,0); const int selected=mic::preferredDevice(list,preferred,hint);
    for(size_t i=0;i<list.size();++i) {
        SendMessageW(box,CB_ADDSTRING,0,reinterpret_cast<LPARAM>(list[i].name.c_str()));
    }
    // Never select the user's speakers implicitly: output requires a cable or explicit choice.
    if(selected>=0) SendMessageW(box,CB_SETCURSEL,selected,0);
}
void refresh() {
    auto input=setting(L"input"),output=setting(L"output");
    if(selection(inputBox)>=0 && static_cast<size_t>(selection(inputBox))<inputs.size()) input=inputs[selection(inputBox)].id;
    if(selection(outputBox)>=0 && static_cast<size_t>(selection(outputBox))<outputs.size()) output=outputs[selection(outputBox)].id;
    auto nextInputs=mic::devices(true),nextOutputs=mic::devices(false);
    inputs=std::move(nextInputs); outputs=std::move(nextOutputs);
    outputs.insert(outputs.begin(),{L"TAG",L"Thin Audio Gateway (demo) - virtual microphone"});
    refill(inputBox,inputs,input,L"HyperX"); refill(outputBox,outputs,output,L"Thin Audio Gateway");
}
void enabled(bool active) {
    for(auto h:{inputBox,outputBox,versionBox,bufferBox,GetDlgItem(window,Refresh)}) EnableWindow(h,!active);
    EnableWindow(muteButton,active);
    EnableWindow(startButton,active || (selection(inputBox)>=0 && selection(outputBox)>=0));
    SetWindowTextW(startButton,active?L"&Остановить":L"&Запустить");
    if(!active) SendMessageW(muteButton,BM_SETCHECK,BST_UNCHECKED,0);
}
mic::Config config() {
    int i=selection(inputBox),o=selection(outputBox);
    if(i<0 || o<0) throw std::runtime_error("Выбранное устройство отключено. Обновите список и выберите микрофон и выход.");
    if(inputs.at(i).name.find(L"CABLE Output")!=std::wstring::npos && outputs.at(o).name.find(L"CABLE In")!=std::wstring::npos)
        throw std::runtime_error("Нельзя направлять CABLE Output обратно в CABLE Input. Выберите физический микрофон.");
    mic::Config c; c.input=inputs.at(i).id; c.output=outputs.at(o).id;
    c.version=selection(versionBox)+1;
    c.intensity=static_cast<float>(SendMessageW(intensitySlider,TBM_GETPOS,0,0))/100;
    c.bufferMs=static_cast<unsigned>(SendMessageW(bufferBox,CB_GETITEMDATA,selection(bufferBox),0));
    c.periodMs=static_cast<unsigned>(GetPrivateProfileIntW(L"audio",L"period_ms",5,settings.c_str()));
    c.sdk=root/L"vendor/nvidia-afx-3.0.0";
    c.tag=c.output==L"TAG"; c.tagSdk=root/L"vendor/tag-2.0.0.1903-demo";
    const auto graphs=setting(L"cuda_graphs",L"-1");
    if(graphs!=L"-1" && graphs!=L"0" && graphs!=L"1")
        throw std::runtime_error("В settings.ini параметр cuda_graphs должен быть -1, 0 или 1.");
    c.cudaGraphs=std::stoi(graphs);
    save(L"input",c.input); save(L"output",c.output); save(L"version",std::to_wstring(c.version));
    save(L"intensity",std::to_wstring(static_cast<int>(c.intensity*100+0.5f))); save(L"buffer_ms",std::to_wstring(c.bufferMs));
    return c;
}
void toggle() {
    if(engine->running()) { engine->stop(); wasRunning=false; enabled(false); }
    else {
        auto c=config();
        auto name=outputs.at(selection(outputBox)).name;
        if(!c.tag && name.find(L"CABLE")==std::wstring::npos && name.find(L"Voicemeeter")==std::wstring::npos) {
            if(MessageBoxW(window,L"Этот выход может воспроизводить ваш микрофон в колонках или наушниках. Продолжить?",L"Прослушивание микрофона",MB_OKCANCEL|MB_ICONWARNING)!=IDOK) return;
        }
        engine->start(c); wasRunning=true; enabled(true);
    }
    InvalidateRect(window,nullptr,FALSE);
}
void show() { ShowWindow(window,SW_RESTORE); SetForegroundWindow(window); }
void addTray() {
    if(!trayAdded) {
        tray.cbSize=sizeof(tray); tray.hWnd=window; tray.uID=1; tray.uFlags=NIF_MESSAGE|NIF_ICON|NIF_TIP;
        tray.uCallbackMessage=trayMessage; tray.hIcon=LoadIconW(nullptr,IDI_APPLICATION);
        const auto tip=L"MicNoiseReducer — "+lastStatus;
        wcsncpy_s(tray.szTip,tip.c_str(),_TRUNCATE);
        if(!Shell_NotifyIconW(NIM_ADD,&tray)) throw std::runtime_error("Не удалось добавить значок в трей.");
        trayAdded=true;
    }
}
void toTray() {
    addTray();
    ShowWindow(window,SW_HIDE);
}
void updateStatus() {
    const bool active=engine->running();
    const auto raw=engine->status();
    std::wstring label,detail;
    const int o=selection(outputBox);
    const bool tag=o>=0 && static_cast<size_t>(o)<outputs.size() && outputs[o].id==L"TAG";
    const bool cable=o>=0 && static_cast<size_t>(o)<outputs.size() && outputs[o].name.find(L"CABLE Input")!=std::wstring::npos;
    if(!active && raw!=L"Stopped") {
        label=L"Ошибка — обработка остановлена";
        detail=raw+L"\r\nПроверьте устройство и повторите запуск. Текст ошибки можно выделить и скопировать.";
    } else if(!active) {
        label=L"Готов к запуску";
        detail=L"«В трей» оставляет обработку включённой. Закрытие окна останавливает микрофон.";
        if(selection(inputBox)<0 || o<0) {
            label=L"Выберите доступный микрофон и выход";
            detail=L"Сохранённое устройство не найдено. Подключите его и нажмите «Обновить».";
        }
    } else if(engine->stats.inputPeriodMs==0) {
        label=L"Загрузка модели NVIDIA…";
        detail=L"Подготовка обработки. Выходной звук появится после загрузки модели.";
    } else {
        label=engine->muted?L"Звук выключен — на выходе тишина":
            (engine->stats.outputActive?L"Работает — шумоподавление включено":L"Готов — ожидает подключения приложения");
        if(tag) detail=L"В Discord / игре выберите: Microphone (Thin Audio Gateway).";
        else if(cable) detail=L"В Discord / игре выберите: CABLE Output (VB-Audio Virtual Cable).";
        else detail=L"Звук направлен на выбранное выходное устройство.";
        if(engine->stats.underruns || engine->stats.drops || engine->stats.tagDriverGaps)
            detail+=L"\r\nВ этой сессии были пропуски. Если слышны обрывы, увеличьте запас буфера.";
    }
    if(label!=lastStatus) {
        lastStatus=label; SetWindowTextW(statusLabel,label.c_str());
        if(trayAdded) {
            const auto tip=L"MicNoiseReducer — "+label;
            wcsncpy_s(tray.szTip,tip.c_str(),_TRUNCATE); tray.uFlags=NIF_TIP;
            Shell_NotifyIconW(NIM_MODIFY,&tray);
        }
    }
    if(detail!=lastDetail) {lastDetail=detail;SetWindowTextW(detailBox,detail.c_str());}
}
void meter(HDC dc,int y,const wchar_t* name,float value) {
    text(dc,48,y,76,22,name,false,mutedInk);
    RECT r{px(132),px(y+5),px(620),px(y+13)};
    HBRUSH base=CreateSolidBrush(RGB(223,232,227)); FillRect(dc,&r,base); DeleteObject(base);
    const float db=20*std::log10(std::max(value,0.000001f));
    r.right=r.left+static_cast<LONG>((r.right-r.left)*std::clamp((db+60)/60,0.0f,1.0f));
    HBRUSH fill=CreateSolidBrush(value>=0.99f?RGB(190,65,45):accent); FillRect(dc,&r,fill); DeleteObject(fill);
    wchar_t label[32];
    if(value<=0.000001f) wcscpy_s(label,L"−∞ dBFS");
    else swprintf_s(label,L"%.0f dBFS",static_cast<double>(db));
    text(dc,628,y,92,22,label,false,value>=0.99f?RGB(190,65,45):mutedInk);
}
void paint() {
    PAINTSTRUCT ps; HDC dc=BeginPaint(window,&ps);
    RECT bounds; GetClientRect(window,&bounds); FillRect(dc,&bounds,backgroundBrush);
    text(dc,32,25,704,38,L"MicNoiseReducer",true);
    text(dc,34,70,700,26,L"Шумоподавление NVIDIA · обработка на вашем ПК",false,mutedInk);
    for(auto r: {RECT{32,112,736,286},RECT{32,302,736,487},RECT{32,503,736,613}}) {
        RECT scaled{px(r.left),px(r.top),px(r.right),px(r.bottom)}; FillRect(dc,&scaled,whiteBrush);
    }
    text(dc,48,123,660,23,L"Микрофон → NVIDIA → приложение",false,accent);
    const int selected=selection(outputBox);
    const bool tag=selected>=0 && static_cast<size_t>(selected)<outputs.size() && outputs[selected].id==L"TAG";
    text(dc,48,460,660,22,L"Запас буфера — защита от обрывов, а не полная задержка.",false,mutedInk);
    meter(dc,518,L"Вход",engine->stats.inputPeak.exchange(0)); meter(dc,548,L"Выход",engine->stats.outputPeak.exchange(0));
    std::wostringstream metrics; metrics<<std::fixed<<std::setprecision(1)<<L"Обработка "<<engine->stats.processMs.load()<<L" мс   ·   Очередь "
        <<engine->stats.outputQueue.load()/48.0<<L" мс   ·   Пропуски "<<engine->stats.underruns.load()<<L"   Сбросы "<<engine->stats.drops.load();
    if(tag && engine->stats.tagDriverGaps) metrics<<L"   TAG "<<engine->stats.tagDriverGaps.load();
    text(dc,48,583,672,22,metrics.str(),false,mutedInk);
    EndPaint(window,&ps);
}
LRESULT CALLBACK procedure(HWND h,UINT message,WPARAM w,LPARAM l) {
    try {
        if(taskbarCreated && message==taskbarCreated && trayAdded) {
            trayAdded=false;
            try {addTray();} catch(...) {show();}
            return 0;
        }
        switch(message) {
        case WM_CREATE: {
            window=h;
            font=CreateFontW(-px(15),0,0,0,FW_NORMAL,FALSE,FALSE,FALSE,DEFAULT_CHARSET,OUT_DEFAULT_PRECIS,CLIP_DEFAULT_PRECIS,CLEARTYPE_QUALITY,DEFAULT_PITCH,L"Segoe UI");
            titleFont=CreateFontW(-px(32),0,0,0,FW_SEMIBOLD,FALSE,FALSE,FALSE,DEFAULT_CHARSET,OUT_DEFAULT_PRECIS,CLIP_DEFAULT_PRECIS,CLEARTYPE_QUALITY,DEFAULT_PITCH,L"Segoe UI");
            control(WC_STATICW,L"&Микрофон",SS_LEFT,48,153,660,22,201);
            inputBox=control(WC_COMBOBOXW,L"Микрофон",CBS_DROPDOWNLIST|WS_TABSTOP|WS_VSCROLL,48,178,672,300,Input);
            control(WC_STATICW,L"&Выход — куда передать обработанный голос",SS_LEFT,48,213,660,22,202);
            outputBox=control(WC_COMBOBOXW,L"Выход",CBS_DROPDOWNLIST|WS_TABSTOP|WS_VSCROLL,48,238,672,300,Output);
            control(WC_STATICW,L"Модель &шумоподавления",SS_LEFT,48,316,324,22,203);
            versionBox=control(WC_COMBOBOXW,L"NVIDIA model",CBS_DROPDOWNLIST|WS_TABSTOP,48,344,326,150,Version);
            SendMessageW(versionBox,CB_ADDSTRING,0,reinterpret_cast<LPARAM>(L"Denoiser v1"));
            SendMessageW(versionBox,CB_ADDSTRING,0,reinterpret_cast<LPARAM>(L"Denoiser v2 — экспериментальный"));
            auto v=GetPrivateProfileIntW(L"audio",L"version",2,settings.c_str()); SendMessageW(versionBox,CB_SETCURSEL,v==1?0:1,0);
            SendMessageW(versionBox,CB_SETDROPPEDWIDTH,px(360),0);
            control(WC_STATICW,L"Запас бу&фера",SS_LEFT,398,316,310,22,204);
            bufferBox=control(WC_COMBOBOXW,L"Queue reserve",CBS_DROPDOWNLIST|WS_TABSTOP,398,344,322,170,Buffer);
            auto b=GetPrivateProfileIntW(L"audio",L"buffer_ms",40,settings.c_str());
            for(unsigned ms:{10u,20u,30u,40u,60u,80u}) {
                auto label=std::to_wstring(ms)+L" мс"+(ms==40?L" — проверенный режим":L""); auto idx=SendMessageW(bufferBox,CB_ADDSTRING,0,reinterpret_cast<LPARAM>(label.c_str()));
                SendMessageW(bufferBox,CB_SETITEMDATA,idx,ms);
                if(ms==b) SendMessageW(bufferBox,CB_SETCURSEL,idx,0);
            }
            if(selection(bufferBox)<0) SendMessageW(bufferBox,CB_SETCURSEL,3,0);
            control(WC_STATICW,L"Сила &подавления",SS_LEFT,48,389,660,22,205);
            intensitySlider=control(TRACKBAR_CLASSW,L"Сила подавления",TBS_AUTOTICKS|TBS_TOOLTIPS|WS_TABSTOP,44,415,680,35,Intensity);
            SendMessageW(intensitySlider,TBM_SETRANGE,TRUE,MAKELPARAM(0,100)); SendMessageW(intensitySlider,TBM_SETTICFREQ,10,0);
            auto strength=std::min(GetPrivateProfileIntW(L"audio",L"intensity",100,settings.c_str()),100u); SendMessageW(intensitySlider,TBM_SETPOS,TRUE,strength);
            SetWindowTextW(GetDlgItem(h,205),(L"Сила &подавления: "+std::to_wstring(strength)+L"%").c_str());
            startButton=control(WC_BUTTONW,L"&Запустить",BS_DEFPUSHBUTTON|WS_TABSTOP,32,633,202,34,Start);
            muteButton=control(WC_BUTTONW,L"&Без звука",BS_AUTOCHECKBOX|WS_TABSTOP,252,638,140,25,Mute);
            control(WC_BUTTONW,L"&Обновить",WS_TABSTOP,410,633,154,34,Refresh);
            control(WC_BUTTONW,L"В &трей",WS_TABSTOP,582,633,154,34,Tray);
            statusLabel=control(WC_STATICW,L"",SS_LEFT,34,676,700,22,206);
            detailBox=control(WC_EDITW,L"",ES_READONLY|ES_MULTILINE|ES_AUTOVSCROLL|WS_VSCROLL|WS_TABSTOP,34,701,700,43,207);
            refresh(); enabled(false); updateStatus(); SetTimer(h,1,200,nullptr); return 0;
        }
        case WM_COMMAND:
            if(HIWORD(w)==CBN_SELCHANGE) {
                const int id=LOWORD(w);
                if(id==Input && selection(inputBox)>=0) save(L"input",inputs.at(selection(inputBox)).id);
                if(id==Output && selection(outputBox)>=0) save(L"output",outputs.at(selection(outputBox)).id);
                if(id==Version) save(L"version",std::to_wstring(selection(versionBox)+1));
                if(id==Buffer) save(L"buffer_ms",std::to_wstring(SendMessageW(bufferBox,CB_GETITEMDATA,selection(bufferBox),0)));
                enabled(engine->running()); updateStatus(); InvalidateRect(h,nullptr,FALSE); return 0;
            }
            if(HIWORD(w)!=BN_CLICKED) return 0;
            switch(LOWORD(w)) {
            case IDOK: case Start: toggle(); break;
            case Mute:
                if(l==0) SendMessageW(muteButton,BM_SETCHECK,engine->muted?BST_UNCHECKED:BST_CHECKED,0);
                engine->muted=SendMessageW(muteButton,BM_GETCHECK,0,0)==BST_CHECKED; break;
            case Refresh: refresh(); enabled(false); break;
            case Output: InvalidateRect(h,nullptr,FALSE); break;
            case Tray: toTray(); break;
            case Show: show(); break;
            case Exit: DestroyWindow(h); break;
            } updateStatus(); InvalidateRect(h,nullptr,FALSE); return 0;
        case WM_HSCROLL: {
            auto value=static_cast<int>(SendMessageW(intensitySlider,TBM_GETPOS,0,0)); engine->intensity=value/100.0f;
            SetWindowTextW(GetDlgItem(h,205),(L"Сила &подавления: "+std::to_wstring(value)+L"%").c_str());
            if(LOWORD(w)!=TB_THUMBTRACK) save(L"intensity",std::to_wstring(value));
            InvalidateRect(h,nullptr,FALSE); return 0;
        }
        case WM_TIMER:
            if(wasRunning && !engine->running()) {
                engine->stop(); enabled(false); InvalidateRect(h,nullptr,FALSE);
                if(!IsWindowVisible(h) || IsIconic(h)) show();
            }
            wasRunning=engine->running();
            updateStatus();
            if(IsWindowVisible(h) && !IsIconic(h) && wasRunning) {
                RECT meters{px(48),px(518),px(720),px(610)}; InvalidateRect(h,&meters,FALSE);
            }
            return 0;
        case trayMessage:
            if(l==WM_LBUTTONUP || l==WM_LBUTTONDBLCLK) show();
            if(l==WM_RBUTTONUP) {
                HMENU menu=CreatePopupMenu(); AppendMenuW(menu,MF_STRING,Show,L"Открыть");
                if(engine->running()) {
                    AppendMenuW(menu,MF_STRING|(engine->muted?MF_CHECKED:0),Mute,L"Без звука");
                    AppendMenuW(menu,MF_STRING,Start,L"Остановить обработку");
                }
                AppendMenuW(menu,MF_STRING,Exit,L"Завершить работу"); POINT p; GetCursorPos(&p); SetForegroundWindow(h);
                TrackPopupMenu(menu,TPM_RIGHTBUTTON,p.x,p.y,0,h,nullptr); DestroyMenu(menu); PostMessageW(h,WM_NULL,0,0);
            } return 0;
        case WM_CTLCOLORSTATIC: case WM_CTLCOLORBTN: {
            const int id=GetDlgCtrlID(reinterpret_cast<HWND>(l));
            bool card=(id>=201 && id<=205) || id==Intensity;
            const bool error=!engine->running() && engine->status()!=L"Stopped";
            SetTextColor(reinterpret_cast<HDC>(w),id==206?(error?RGB(170,42,32):accent):ink); SetBkColor(reinterpret_cast<HDC>(w),card?RGB(255,255,255):background);
            return reinterpret_cast<LRESULT>(card?whiteBrush:backgroundBrush);
        }
        case WM_ERASEBKGND: return 1;
        case WM_PAINT: paint(); return 0;
        case WM_CLOSE: DestroyWindow(h); return 0;
        case WM_DESTROY:
            KillTimer(h,1); engine->stop(); if(trayAdded) Shell_NotifyIconW(NIM_DELETE,&tray); PostQuitMessage(0); return 0;
        }
    } catch(const std::exception& e) { MessageBoxW(h,mic::wide(e.what()).c_str(),L"MicNoiseReducer",MB_OK|MB_ICONERROR); if(message==WM_CREATE) return -1; }
    return DefWindowProcW(h,message,w,l);
}
}
int WINAPI wWinMain(HINSTANCE instance,HINSTANCE,PWSTR,int showCommand) {
    HANDLE single=CreateMutexW(nullptr,FALSE,L"Local\\MicNoiseReducer.SingleInstance");
    if(GetLastError()==ERROR_ALREADY_EXISTS) { if(auto h=FindWindowW(L"MicNoiseReducer.Window",nullptr)) { ShowWindow(h,SW_RESTORE); SetForegroundWindow(h); } if(single) CloseHandle(single); return 0; }
    int exitCode=0;
    try {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE); dpi=GetDpiForSystem();
        RECT work{};
        if(SystemParametersInfoW(SPI_GETWORKAREA,0,&work,0)) dpi=std::min(dpi,static_cast<UINT>(std::max(72L,(work.bottom-work.top-60)*96/760)));
        root=mic::projectRoot(); settings=root/L"settings.ini"; engine=std::make_unique<mic::Engine>();
        INITCOMMONCONTROLSEX controls{sizeof(controls),ICC_STANDARD_CLASSES|ICC_BAR_CLASSES}; InitCommonControlsEx(&controls);
        backgroundBrush=CreateSolidBrush(background); whiteBrush=CreateSolidBrush(RGB(255,255,255));
        WNDCLASSW c{}; c.hInstance=instance; c.lpfnWndProc=procedure; c.lpszClassName=L"MicNoiseReducer.Window";
        c.hCursor=LoadCursorW(nullptr,IDC_ARROW); c.hIcon=LoadIconW(nullptr,IDI_APPLICATION); c.hbrBackground=backgroundBrush;
        if(!RegisterClassW(&c)) throw std::runtime_error("Window class registration failed");
        RECT size{0,0,px(768),px(760)}; constexpr DWORD style=WS_OVERLAPPED|WS_CAPTION|WS_SYSMENU|WS_MINIMIZEBOX|WS_CLIPCHILDREN;
        AdjustWindowRectEx(&size,style,FALSE,0);
        HWND h=CreateWindowExW(WS_EX_CONTROLPARENT,c.lpszClassName,L"MicNoiseReducer",style,CW_USEDEFAULT,CW_USEDEFAULT,size.right-size.left,size.bottom-size.top,nullptr,nullptr,instance,nullptr);
        if(!h) throw std::runtime_error("Window creation failed");
        ShowWindow(h,showCommand); UpdateWindow(h);
        MSG message; while(GetMessageW(&message,nullptr,0,0)>0) { if(!IsDialogMessageW(h,&message)) { TranslateMessage(&message); DispatchMessageW(&message); } }
    } catch(const std::exception& e) { MessageBoxW(nullptr,mic::wide(e.what()).c_str(),L"MicNoiseReducer",MB_OK|MB_ICONERROR); exitCode=1; }
    engine.reset(); if(font) DeleteObject(font); if(titleFont) DeleteObject(titleFont);
    if(backgroundBrush) DeleteObject(backgroundBrush); if(whiteBrush) DeleteObject(whiteBrush); if(single) CloseHandle(single);
    return exitCode;
}
