@echo off
rem Builds dist\ALS.dll (the 32-bit TunerPro plug-in), dist\ALS\ALS.exe (the program it opens)
rem and dist\ALS-TunerPro.zip (both, with install.bat, for a release).
rem Needs Rust, and Visual Studio with the C++ desktop tools.
setlocal
cd /d "%~dp0"

if not exist obj mkdir obj
if not exist dist\ALS mkdir dist\ALS

rem The program first: cargo finds its own 64-bit linker, which the 32-bit setup below would hide.
cargo build --release --manifest-path app\Cargo.toml
if errorlevel 1 exit /b 1
copy /y app\target\release\ALS.exe dist\ALS\ALS.exe >nul
if errorlevel 1 exit /b 1

set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSWHERE%" (
  echo vswhere.exe not found. Install Visual Studio with the C++ desktop tools.
  exit /b 1
)
rem vcvars32.bat calls vswhere itself and expects it on the path.
set "PATH=%PATH%;%ProgramFiles(x86)%\Microsoft Visual Studio\Installer"
for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -property installationPath`) do set "VSDIR=%%i"
call "%VSDIR%\VC\Auxiliary\Build\vcvars32.bat" >nul
if errorlevel 1 exit /b 1

set LIBS=user32.lib comdlg32.lib comctl32.lib shell32.lib
cl /nologo /std:c++17 /utf-8 /O2 /MT /EHsc /W3 /DNDEBUG /DUNICODE /D_UNICODE /D_CRT_SECURE_NO_WARNINGS /Fo:obj\ plugin\plugin.cpp /LD /Fe:dist\ALS.dll /link %LIBS%
if errorlevel 1 exit /b 1
del /q dist\ALS.exp dist\ALS.lib 2>nul

rem The zip people download: the plug-in, the program in its folder, and the installer.
powershell -NoProfile -Command "Compress-Archive -Force -Path 'dist\ALS.dll','dist\ALS','install.bat' -DestinationPath 'dist\ALS-TunerPro.zip'"
if errorlevel 1 exit /b 1

echo Built dist\ALS.dll, dist\ALS\ALS.exe and dist\ALS-TunerPro.zip
