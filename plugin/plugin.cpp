// TunerPro plug-in "ALS", developed by AlexDev.
//
// Adds an "ALS" menu to TunerPro's main window. The menu starts ALS.exe, the map workbench
// that sits in the "ALS" folder next to this DLL, with the bin and the XDF TunerPro has open.
// When ALS sends a finished XDF back, this plug-in has TunerPro load it.
//
// TunerPro loads every DLL in "Documents\TunerPro Files\Plugins" that exports TPCreatePlugin
// and TPReleasePlugin, and talks to the object through a vtable. This plug-in offers no
// hardware component: it only needs to be loaded, and then puts its own menu on the bar.
#include <windows.h>

#include <commctrl.h>
#include <commdlg.h>
#include <shellapi.h>
#include <shlobj.h>

#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwctype>
#include <string>
#include <vector>

namespace {

const wchar_t kName[] = L"ALS";
const char kVersion[] = "1.0.1";
// The mark ALS.exe puts on the message that carries an XDF path: "ADXF".
const ULONG_PTR kLoadXdf = 0x41445846;
const UINT_PTR kSubclassId = 0x414C5331;  // "ALS1"
const UINT_PTR kWatchdogTimer = 0x414C;

HINSTANCE g_instance = nullptr;
HANDLE g_watcher = nullptr;
volatile LONG g_stop = 0;
HWND g_main = nullptr;
HMENU g_popup = nullptr;
UINT g_first_command = 0;
HHOOK g_install_hook = nullptr;
HWND g_install_target = nullptr;
FILE* g_log = nullptr;

void Log(const char* format, ...) {
    if (!g_log) return;
    SYSTEMTIME t;
    GetLocalTime(&t);
    fprintf(g_log, "%02d:%02d:%02d.%03d  ", t.wHour, t.wMinute, t.wSecond, t.wMilliseconds);
    va_list args;
    va_start(args, format);
    vfprintf(g_log, format, args);
    va_end(args);
    fputc('\n', g_log);
    fflush(g_log);
}

// The folder this DLL is in, with the closing backslash.
std::wstring ModuleDir() {
    wchar_t path[MAX_PATH] = {0};
    GetModuleFileNameW(g_instance, path, MAX_PATH);
    wchar_t* slash = wcsrchr(path, L'\\');
    if (slash) slash[1] = 0;
    return path;
}

void OpenLog() {
    const std::wstring path = ModuleDir() + L"ALS.log";
    g_log = _wfsopen(path.c_str(), L"w", _SH_DENYNO);
}

// ---------- text ----------

std::wstring BaseName(const std::wstring& path) {
    const size_t slash = path.find_last_of(L"\\/");
    return slash == std::wstring::npos ? path : path.substr(slash + 1);
}

bool EqualNoCase(const std::wstring& a, const std::wstring& b) {
    return _wcsicmp(a.c_str(), b.c_str()) == 0;
}

bool EndsWithNoCase(const std::wstring& text, const std::wstring& end) {
    return text.size() >= end.size() && _wcsicmp(text.c_str() + text.size() - end.size(), end.c_str()) == 0;
}

bool Exists(const std::wstring& path) {
    const DWORD a = GetFileAttributesW(path.c_str());
    return a != INVALID_FILE_ATTRIBUTES && !(a & FILE_ATTRIBUTE_DIRECTORY);
}

bool IsAbsolute(const std::wstring& path) {
    return (path.size() > 2 && path[1] == L':') || path.compare(0, 2, L"\\\\") == 0;
}

std::string Narrow(const std::wstring& s) {
    const int n = WideCharToMultiByte(CP_UTF8, 0, s.c_str(), -1, nullptr, 0, nullptr, nullptr);
    if (n <= 1) return "";
    std::string out(n - 1, '\0');
    WideCharToMultiByte(CP_UTF8, 0, s.c_str(), -1, &out[0], n, nullptr, nullptr);
    return out;
}

// ---------- TunerPro's settings file ----------

// A file from "Documents\TunerPro Files", whole. Empty when it isn't there.
std::string ReadTunerProFile(const wchar_t* name) {
    wchar_t docs[MAX_PATH] = {0};
    if (FAILED(SHGetFolderPathW(nullptr, CSIDL_PERSONAL, nullptr, SHGFP_TYPE_CURRENT, docs))) return "";
    const std::wstring path = std::wstring(docs) + L"\\TunerPro Files\\" + name;
    FILE* f = _wfopen(path.c_str(), L"rb");
    if (!f) return "";
    std::string text;
    char buf[8192];
    size_t got;
    while ((got = fread(buf, 1, sizeof(buf), f)) > 0) text.append(buf, got);
    fclose(f);
    return text;
}

// TunerPro 6 keeps its settings as JSON.
std::string ReadSettingsFile() {
    return ReadTunerProFile(L"TunerProSettings.json");
}

// TunerPro 5 keeps its settings in a binary file, with the paths of recent files as plain
// zero-terminated text in the system code page. Collects every absolute path in it.
std::vector<std::wstring> DatPaths() {
    std::vector<std::wstring> paths;
    const std::string data = ReadTunerProFile(L"TunerProSettings.dat");
    for (size_t i = 0; i + 3 < data.size(); ++i) {
        const bool starts = i == 0 || data[i - 1] == 0;
        if (!starts || !isalpha(static_cast<unsigned char>(data[i])) || data[i + 1] != ':' || data[i + 2] != '\\') continue;
        size_t end = i;
        while (end < data.size() && data[end] != 0 && end - i < MAX_PATH) ++end;
        if (end < data.size() && data[end] == 0) {
            const std::string s = data.substr(i, end - i);
            const int n = MultiByteToWideChar(CP_ACP, 0, s.c_str(), -1, nullptr, 0);
            if (n > 1) {
                std::wstring w(n - 1, L'\0');
                MultiByteToWideChar(CP_ACP, 0, s.c_str(), -1, &w[0], n);
                paths.push_back(w);
            }
        }
        i = end;
    }
    return paths;
}

// TunerPro is an ANSI program, so paths in its settings file can be in the system code page.
std::wstring DecodeSettingsString(const std::string& s) {
    int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, s.c_str(), -1, nullptr, 0);
    UINT cp = CP_UTF8;
    if (n <= 0) {
        cp = CP_ACP;
        n = MultiByteToWideChar(CP_ACP, 0, s.c_str(), -1, nullptr, 0);
    }
    if (n <= 1) return L"";
    std::wstring out(n - 1, L'\0');
    MultiByteToWideChar(cp, 0, s.c_str(), -1, &out[0], n);
    return out;
}

// Reads the JSON string that starts at the opening quote.
std::string ParseJsonString(const std::string& text, size_t quote, size_t* end) {
    std::string out;
    size_t i = quote + 1;
    while (i < text.size() && text[i] != '"') {
        if (text[i] == '\\' && i + 1 < text.size()) {
            ++i;
            switch (text[i]) {
                case 'n': out += '\n'; break;
                case 't': out += '\t'; break;
                default: out += text[i]; break;
            }
        } else {
            out += text[i];
        }
        ++i;
    }
    if (end) *end = i + 1;
    return out;
}

// The paths TunerPro wrote down under a "last..." key and a "recent..." list.
std::vector<std::wstring> SettingsPaths(const char* last_key, const char* recent_key) {
    std::vector<std::wstring> paths;
    const std::string text = ReadSettingsFile();
    size_t at = text.find(std::string("\"") + last_key + "\"");
    if (at != std::string::npos) {
        const size_t quote = text.find('"', text.find(':', at));
        if (quote != std::string::npos) paths.push_back(DecodeSettingsString(ParseJsonString(text, quote, nullptr)));
    }
    at = text.find(std::string("\"") + recent_key + "\"");
    if (at != std::string::npos) {
        size_t pos = text.find('[', at);
        const size_t close = text.find(']', pos);
        while (pos != std::string::npos && close != std::string::npos) {
            const size_t quote = text.find('"', pos);
            if (quote == std::string::npos || quote > close) break;
            paths.push_back(DecodeSettingsString(ParseJsonString(text, quote, &pos)));
        }
    }
    return paths;
}

// Follows TunerPro's own language: the "language" setting picks lang\<code>.txt next to the
// exe. It is Thai if the code is "th", or if that file is one the tunerpro-thai installer
// rewrote, which marks it on line one.
bool IsThai() {
    static int cached = -1;
    if (cached >= 0) return cached != 0;
    cached = 0;
    std::string code = "en";
    const std::string text = ReadSettingsFile();
    const size_t at = text.find("\"language\"");
    if (at != std::string::npos) {
        const size_t quote = text.find('"', text.find(':', at));
        if (quote != std::string::npos) code = ParseJsonString(text, quote, nullptr);
    }
    if (code == "th") {
        cached = 1;
        return true;
    }
    wchar_t exe[MAX_PATH] = {0};
    GetModuleFileNameW(nullptr, exe, MAX_PATH);
    wchar_t* slash = wcsrchr(exe, L'\\');
    if (slash) *slash = 0;
    const std::wstring lang = std::wstring(exe) + L"\\lang\\" + DecodeSettingsString(code) + L".txt";
    if (FILE* f = _wfopen(lang.c_str(), L"rb")) {
        char head[64] = {0};
        fread(head, 1, sizeof(head) - 1, f);
        fclose(f);
        if (strstr(head, "# TunerPro-Thai")) cached = 1;
    }
    return cached != 0;
}

const wchar_t* Say(const wchar_t* thai, const wchar_t* english) {
    return IsThai() ? thai : english;
}

// ---------- which files TunerPro has open ----------

struct OpenNames {
    std::wstring bin, xdf;
    bool unsaved = false;
};

// TunerPro's title reads "TunerPro RT - name.bin* | XDF File - name.xdf* | ADX File - ...",
// with a star after a file that has unsaved changes.
OpenNames TitleNames() {
    OpenNames names;
    wchar_t title[1024] = {0};
    GetWindowTextW(g_main, title, 1024);
    std::wstring rest = title;
    const std::wstring prefix = L"TunerPro RT - ";
    if (rest.compare(0, prefix.size(), prefix) != 0) return names;
    rest = rest.substr(prefix.size());
    auto clean = [&names](std::wstring s) {
        while (!s.empty() && (s.back() == L' ' || s.back() == L'*')) {
            if (s.back() == L'*') names.unsaved = true;
            s.pop_back();
        }
        return s;
    };
    const std::wstring xdf = L"XDF File - ", adx = L"ADX File - ";
    bool first = true;
    for (size_t from = 0;;) {
        const size_t bar = rest.find(L" | ", from);
        const std::wstring part = rest.substr(from, bar == std::wstring::npos ? std::wstring::npos : bar - from);
        if (part.compare(0, xdf.size(), xdf) == 0) {
            names.xdf = clean(part.substr(xdf.size()));
        } else if (part.compare(0, adx.size(), adx) != 0 && first) {
            names.bin = clean(part);
        }
        first = false;
        if (bar == std::wstring::npos) break;
        from = bar + 3;
    }
    // What TunerPro shows in the bin's place when there is none.
    if (EqualNoCase(names.bin, L"No Bin Selected")) names.bin.clear();
    return names;
}

// TunerPro's recent-file menus read "2 64kb\K03-H01\K03-H01.bin": a number, then the end of
// the path. Collects the ones for this file name, in menu order.
void CollectMenuFiles(HMENU menu, const std::wstring& name, std::vector<std::wstring>* out, int depth = 0) {
    if (!menu || depth > 4) return;
    const int n = GetMenuItemCount(menu);
    for (int i = 0; i < n; ++i) {
        if (HMENU sub = GetSubMenu(menu, i)) {
            CollectMenuFiles(sub, name, out, depth + 1);
            continue;
        }
        wchar_t text[600] = {0};
        if (GetMenuStringW(menu, i, text, 600, MF_BYPOSITION) <= 0) continue;
        const wchar_t* p = text;
        if (*p == L'&') ++p;
        if (!iswdigit(*p)) continue;
        while (iswdigit(*p)) ++p;
        if (*p != L' ') continue;
        const std::wstring tail = p + 1;
        if (EqualNoCase(BaseName(tail), name)) out->push_back(tail);
    }
}

// Files the person pointed out when a path couldn't be worked out, for the rest of the session.
std::vector<std::wstring> g_chosen;

// The full path of a file TunerPro has open, from its name. The title only carries the name;
// the folder comes from the recent-file menu and the paths in TunerPro's settings (which it
// writes when it closes, so a file opened for the first time isn't in them yet). Empty when
// that doesn't settle on one file.
std::wstring ResolveOpenFile(const std::wstring& name, const char* last_key, const char* recent_key) {
    if (name.empty()) return L"";
    for (const std::wstring& path : g_chosen) {
        if (EqualNoCase(BaseName(path), name) && Exists(path)) return path;
    }
    std::vector<std::wstring> tails;
    CollectMenuFiles(GetMenu(g_main), name, &tails);
    std::vector<std::wstring> recorded = SettingsPaths(last_key, recent_key);
    const std::vector<std::wstring> dat = DatPaths();
    recorded.insert(recorded.end(), dat.begin(), dat.end());
    std::vector<std::wstring> known;
    for (const std::wstring& path : recorded) {
        if (!EqualNoCase(BaseName(path), name) || !Exists(path)) continue;
        bool twice = false;
        for (const std::wstring& k : known) twice = twice || EqualNoCase(k, path);
        if (!twice) known.push_back(path);
    }
    for (const std::wstring& tail : tails) {
        if (IsAbsolute(tail) && Exists(tail)) return tail;
        for (const std::wstring& path : known) {
            if (EndsWithNoCase(path, L"\\" + tail)) return path;
        }
    }
    if (tails.empty() && known.size() == 1) return known[0];
    return L"";
}

std::wstring AskForFile(HWND owner, const std::wstring& name) {
    wchar_t file[MAX_PATH] = {0};
    wcsncpy_s(file, name.c_str(), _TRUNCATE);
    const std::wstring title = std::wstring(Say(L"ALS: เลือกไฟล์ที่เปิดอยู่ใน TunerPro - ", L"ALS: choose the file TunerPro has open - ")) + name;
    OPENFILENAMEW ofn = {sizeof(ofn)};
    ofn.hwndOwner = owner;
    ofn.lpstrFilter = L"Bin (*.bin)\0*.bin\0All files (*.*)\0*.*\0";
    ofn.lpstrFile = file;
    ofn.nMaxFile = MAX_PATH;
    ofn.lpstrTitle = title.c_str();
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR;
    return GetOpenFileNameW(&ofn) ? std::wstring(file) : std::wstring();
}

// ---------- starting ALS, and taking its XDF ----------

void StartAls(HWND owner, bool with_files) {
    const std::wstring dir = ModuleDir() + L"ALS\\";
    const std::wstring exe = dir + L"ALS.exe";
    if (!Exists(exe)) {
        const std::wstring text = std::wstring(Say(L"ไม่พบโปรแกรม ALS ที่\n", L"ALS was not found at\n")) + exe;
        MessageBoxW(owner, text.c_str(), kName, MB_OK | MB_ICONWARNING);
        return;
    }
    std::wstring cmd = L"\"" + exe + L"\" --tunerpro " + std::to_wstring(static_cast<unsigned long long>(reinterpret_cast<UINT_PTR>(g_main)));
    if (with_files) {
        const OpenNames names = TitleNames();
        if (names.bin.empty()) {
            MessageBoxW(owner, Say(L"ยังไม่ได้เปิดไฟล์ bin ใน TunerPro", L"No bin is open in TunerPro."), kName, MB_OK | MB_ICONINFORMATION);
            return;
        }
        if (names.unsaved) {
            const wchar_t* text = Say(L"ไฟล์ใน TunerPro มีการแก้ไขที่ยังไม่ได้เซฟ\nALS จะเปิดไฟล์ตามที่เซฟไว้ล่าสุด ดำเนินการต่อไหม",
                                      L"TunerPro has unsaved changes.\nALS opens the files as they were last saved. Continue?");
            if (MessageBoxW(owner, text, kName, MB_OKCANCEL | MB_ICONINFORMATION) != IDOK) return;
        }
        std::wstring bin = ResolveOpenFile(names.bin, "lastBinPath", "recentBins");
        if (bin.empty()) {
            bin = AskForFile(owner, names.bin);
            if (bin.empty()) return;
            g_chosen.push_back(bin);
        }
        const std::wstring xdf = ResolveOpenFile(names.xdf, "lastXDFPath", "recentXDFs");
        Log("open: bin=%s xdf=%s", Narrow(bin).c_str(), Narrow(xdf).c_str());
        if (!xdf.empty()) cmd += L" --xdf \"" + xdf + L"\"";
        cmd += L" \"" + bin + L"\"";
    }
    STARTUPINFOW si = {sizeof(si)};
    PROCESS_INFORMATION pi = {};
    std::vector<wchar_t> line(cmd.begin(), cmd.end());
    line.push_back(0);
    if (!CreateProcessW(exe.c_str(), line.data(), nullptr, nullptr, FALSE, 0, nullptr, dir.c_str(), &si, &pi)) {
        const std::wstring text = std::wstring(Say(L"เปิดโปรแกรม ALS ไม่ได้ (รหัส ", L"ALS could not be started (code ")) + std::to_wstring(GetLastError()) + L")";
        MessageBoxW(owner, text.c_str(), kName, MB_OK | MB_ICONWARNING);
        return;
    }
    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);
}

// Hands TunerPro a file the way Explorer does when one is dragged onto the window. TunerPro
// loads it on its own time, and asks about unsaved changes itself.
bool DropFile(HWND window, const std::wstring& path) {
    const size_t chars = path.size() + 2;
    HGLOBAL mem = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, sizeof(DROPFILES) + chars * sizeof(wchar_t));
    if (!mem) return false;
    auto* drop = static_cast<DROPFILES*>(GlobalLock(mem));
    drop->pFiles = sizeof(DROPFILES);
    drop->fWide = TRUE;
    memcpy(reinterpret_cast<uint8_t*>(drop) + sizeof(DROPFILES), path.c_str(), path.size() * sizeof(wchar_t));
    GlobalUnlock(mem);
    if (!PostMessageW(window, WM_DROPFILES, reinterpret_cast<WPARAM>(mem), 0)) {
        GlobalFree(mem);
        return false;
    }
    return true;
}

// ALS.exe sends the path of the XDF it wrote. Returns whether TunerPro was handed the file.
bool TakeXdf(HWND window, const COPYDATASTRUCT* data) {
    if (!data || data->dwData != kLoadXdf || !data->lpData) return false;
    if (data->cbData < 2 * sizeof(wchar_t) || data->cbData > 32768 * sizeof(wchar_t)) return false;
    std::wstring path(static_cast<const wchar_t*>(data->lpData), data->cbData / sizeof(wchar_t));
    path = path.c_str();  // up to the closing zero
    Log("xdf from ALS: %s", Narrow(path).c_str());
    if (!EndsWithNoCase(path, L".xdf") || !Exists(path)) return false;
    return DropFile(window, path);
}

// ---------- the menu ----------

enum Item { kOpenCurrent, kOpenEmpty, kAbout, kItemCount };

struct MenuItemDef {
    Item item;
    const wchar_t* thai;
    const wchar_t* english;
};
const MenuItemDef kItems[] = {
    {kOpenCurrent, L"เปิด ALS กับไฟล์ที่เปิดอยู่ (bin + XDF)", L"Open ALS with the current bin and XDF"},
    {kOpenEmpty, L"เปิด ALS เปล่า", L"Open ALS (empty)"},
    {kAbout, L"เกี่ยวกับ ALS...", L"About ALS..."},
};

bool MenuHasId(HMENU menu, UINT id) {
    const int n = GetMenuItemCount(menu);
    for (int i = 0; i < n; ++i) {
        HMENU sub = GetSubMenu(menu, i);
        if (sub) {
            if (MenuHasId(sub, id)) return true;
        } else if (GetMenuItemID(menu, i) == id) {
            return true;
        }
    }
    return false;
}

// Command ids must not collide with TunerPro's or another plug-in's.
UINT FindFreeCommandBlock(HMENU menu) {
    for (UINT base = 0xBD00; base > 0x9000; base -= 0x100) {
        bool free = true;
        for (int i = 0; i < kItemCount && free; ++i) free = !MenuHasId(menu, base + i);
        if (free) return base;
    }
    return 0xBD00;
}

bool MenuContainsPopup(HMENU menu, HMENU popup) {
    const int n = GetMenuItemCount(menu);
    for (int i = 0; i < n; ++i) {
        if (GetSubMenu(menu, i) == popup) return true;
    }
    return false;
}

void AddMenu() {
    HMENU bar = GetMenu(g_main);
    if (!bar) return;
    if (g_popup && MenuContainsPopup(bar, g_popup)) return;
    if (!g_popup) {
        g_first_command = FindFreeCommandBlock(bar);
        g_popup = CreatePopupMenu();
        for (int i = 0; i < kItemCount; ++i) {
            if (kItems[i].item == kAbout) AppendMenuW(g_popup, MF_SEPARATOR, 0, nullptr);
            AppendMenuW(g_popup, MF_STRING, g_first_command + i, Say(kItems[i].thai, kItems[i].english));
        }
    }
    // In front of the last item, which is Help on TunerPro's bar.
    const int count = GetMenuItemCount(bar);
    InsertMenuW(bar, count > 0 ? count - 1 : 0, MF_BYPOSITION | MF_POPUP | MF_STRING, reinterpret_cast<UINT_PTR>(g_popup), kName);
    DrawMenuBar(g_main);
    Log("menu added, %d items on the bar, first command 0x%X", count + 1, g_first_command);
}

void RunItem(HWND owner, Item item) {
    switch (item) {
        case kOpenCurrent: StartAls(owner, true); break;
        case kOpenEmpty: StartAls(owner, false); break;
        case kAbout:
            MessageBoxW(owner,
                        Say(L"ALS 1.0.1\nพัฒนาโดย AlexDev\n\nเปิดไฟล์ที่ TunerPro เปิดอยู่ในโปรแกรม ALS เพื่อหาแมพ ครอบแมพ และปรับจูน\nแล้วส่งแมพกลับมาเป็น XDF",
                            L"ALS 1.0.1\nDeveloped by AlexDev\n\nOpens the files TunerPro has open in ALS, to find, mark and tune maps,\nand takes the maps back as an XDF."),
                        kName, MB_OK | MB_ICONINFORMATION);
            break;
        default: break;
    }
}

LRESULT CALLBACK MainSubclass(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp, UINT_PTR, DWORD_PTR) {
    switch (msg) {
        case WM_COMMAND: {
            const UINT id = LOWORD(wp);
            if (g_first_command && id >= g_first_command && id < g_first_command + kItemCount) {
                RunItem(hwnd, kItems[id - g_first_command].item);
                return 0;
            }
            break;
        }
        case WM_COPYDATA: {
            const COPYDATASTRUCT* data = reinterpret_cast<const COPYDATASTRUCT*>(lp);
            if (data && data->dwData == kLoadXdf) return TakeXdf(hwnd, data) ? TRUE : FALSE;
            break;
        }
        case WM_NCDESTROY:
            KillTimer(hwnd, kWatchdogTimer);
            RemoveWindowSubclass(hwnd, MainSubclass, kSubclassId);
            g_main = nullptr;
            break;
        default: break;
    }
    return DefSubclassProc(hwnd, msg, wp, lp);
}

// TunerPro can rebuild its menu bar, so put the entry back if it disappears.
void CALLBACK MenuWatchdog(HWND, UINT, UINT_PTR, DWORD) {
    if (g_main) AddMenu();
}

// Runs on TunerPro's UI thread, which is where subclassing and menu changes belong.
LRESULT CALLBACK InstallHook(int code, WPARAM wp, LPARAM lp) {
    if (code >= 0 && g_install_target && !g_main) {
        const CWPSTRUCT* cwp = reinterpret_cast<CWPSTRUCT*>(lp);
        if (cwp->hwnd == g_install_target) {
            g_main = g_install_target;
            SetWindowSubclass(g_main, MainSubclass, kSubclassId, 0);
            AddMenu();
            SetTimer(g_main, kWatchdogTimer, 1500, MenuWatchdog);
        }
    }
    return CallNextHookEx(g_install_hook, code, wp, lp);
}

BOOL CALLBACK FindMainWindowProc(HWND hwnd, LPARAM param) {
    DWORD pid = 0;
    GetWindowThreadProcessId(hwnd, &pid);
    if (pid != GetCurrentProcessId() || !IsWindowVisible(hwnd) || !GetMenu(hwnd)) return TRUE;
    if (GetWindow(hwnd, GW_OWNER)) return TRUE;
    *reinterpret_cast<HWND*>(param) = hwnd;
    return FALSE;
}

DWORD WINAPI WatcherThread(LPVOID) {
    while (!g_stop && !g_main) {
        HWND found = nullptr;
        EnumWindows(FindMainWindowProc, reinterpret_cast<LPARAM>(&found));
        if (found) {
            wchar_t cls[64] = {0};
            GetClassNameW(found, cls, 64);
            Log("main window %p class=%ls", found, cls);
            g_install_target = found;
            const DWORD thread = GetWindowThreadProcessId(found, nullptr);
            g_install_hook = SetWindowsHookExW(WH_CALLWNDPROC, InstallHook, g_instance, thread);
            DWORD_PTR result = 0;
            SendMessageTimeoutW(found, WM_NULL, 0, 0, SMTO_ABORTIFHUNG, 3000, &result);
            if (g_install_hook) UnhookWindowsHookEx(g_install_hook);
            g_install_hook = nullptr;
            g_install_target = nullptr;
        }
        if (!g_main) Sleep(250);
    }
    return 0;
}

// ---------- what TunerPro sees ----------

// What TunerPro asks a plug-in about itself. The layout is TunerPro's.
struct PluginInfo {
    DWORD size;
    DWORD contract_version;
    GUID id;
    char name[50];
    char description[512];
    char version[15];
    char author[100];
    DWORD component_count;
    BOOL configurable;
    WORD version_major;
    WORD version_minor;
};
static_assert(sizeof(PluginInfo) == 0x2CC, "TunerPro expects this layout");

// The object TunerPro talks to: an MSVC thiscall vtable in exactly this slot order.
class AlsPlugin {
public:
    virtual ~AlsPlugin() {}
    virtual BOOL __thiscall Describe(PluginInfo* info) {
        Log("describe(%p) size=0x%lX", info, info ? info->size : 0);
        if (!info || info->size < sizeof(PluginInfo)) return FALSE;
        // {DB55056A-A452-42B9-91BA-8794CCD0FD45}
        static const GUID guid = {0xdb55056a, 0xa452, 0x42b9, {0x91, 0xba, 0x87, 0x94, 0xcc, 0xd0, 0xfd, 0x45}};
        info->contract_version = 3;
        info->id = guid;
        strcpy_s(info->name, "ALS");
        strcpy_s(info->description, "Opens the bin and XDF in ALS to find, mark and tune maps, and takes the maps back as an XDF.");
        strcpy_s(info->version, kVersion);
        strcpy_s(info->author, "AlexDev");
        // TunerPro drops a plug-in that reports no component at all; it asks for this one
        // and is told there is none.
        info->component_count = 1;
        info->configurable = FALSE;
        info->version_major = 1;
        info->version_minor = 0;
        return TRUE;
    }
    virtual HRESULT __thiscall Setup(void*) { return S_OK; }
    virtual void* __thiscall Component(UINT index) {
        Log("component(%u)", index);
        return nullptr;
    }
    virtual void __thiscall DropComponent(void*) {}
    // Three arguments: the callee cleans the stack, so the count has to match TunerPro's.
    virtual LONG_PTR __thiscall Message(UINT message, LPARAM, LPARAM) {
        Log("message %u", message);
        return 0;
    }
};

AlsPlugin* g_plugin = nullptr;

}  // namespace

extern "C" {

__declspec(dllexport) void* TPCreatePlugin() {
    if (!g_plugin) {
        // TunerPro may unload a plug-in that offers no component, but the menu lives on.
        HMODULE self = nullptr;
        GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN, reinterpret_cast<LPCWSTR>(&TPCreatePlugin), &self);
        OpenLog();
        Log("TPCreatePlugin on thread %lu", GetCurrentThreadId());
        g_plugin = new AlsPlugin();
        g_watcher = CreateThread(nullptr, 0, WatcherThread, nullptr, 0, nullptr);
    }
    return g_plugin;
}

__declspec(dllexport) void TPReleasePlugin() {
    Log("TPReleasePlugin");
    delete g_plugin;
    g_plugin = nullptr;
}

}  // extern "C"

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) {
        g_instance = instance;
        DisableThreadLibraryCalls(instance);
    }
    return TRUE;
}
