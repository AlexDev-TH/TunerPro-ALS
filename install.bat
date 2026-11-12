@echo off
rem Copies ALS into TunerPro's plug-in folder. Close TunerPro and ALS first: TunerPro loads
rem plug-ins when it starts and keeps ALS.dll open while it runs.
rem Works from the unpacked release zip (ALS.dll next to this file) and from the source
rem folder after build.bat (ALS.dll in dist). Add /quiet to skip the pause at the end.
setlocal
cd /d "%~dp0"

set "SRC=%~dp0"
if not exist "%SRC%ALS.dll" set "SRC=%~dp0dist\"
if not exist "%SRC%ALS.dll" (
  echo ALS.dll was not found. Unpack the whole zip first, or run build.bat.
  goto failed
)
if not exist "%SRC%ALS\ALS.exe" (
  echo The ALS folder with ALS.exe was not found next to ALS.dll. Unpack the whole zip first.
  goto failed
)

for /f "usebackq delims=" %%i in (`powershell -NoProfile -Command "[Environment]::GetFolderPath('MyDocuments')"`) do set "DOCS=%%i"
set "PLUGINS=%DOCS%\TunerPro Files\Plugins"
if not exist "%PLUGINS%" (
  echo "%PLUGINS%" was not found. Start TunerPro once so it creates its folders, then run this again.
  goto failed
)

if not exist "%PLUGINS%\ALS" mkdir "%PLUGINS%\ALS"
copy /y "%SRC%ALS.dll" "%PLUGINS%\ALS.dll" >nul
if errorlevel 1 (
  echo ALS.dll could not be replaced. Close TunerPro and run this again.
  goto failed
)
copy /y "%SRC%ALS\ALS.exe" "%PLUGINS%\ALS\ALS.exe" >nul
if errorlevel 1 (
  echo ALS.exe could not be replaced. Close the ALS window and run this again.
  goto failed
)

echo Installed into "%PLUGINS%".
echo Start TunerPro and look for the ALS menu.
if /i not "%~1"=="/quiet" pause
exit /b 0

:failed
if /i not "%~1"=="/quiet" pause
exit /b 1
