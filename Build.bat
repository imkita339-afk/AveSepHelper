@echo off
setlocal
cd /d "%~dp0"
cargo build --release --locked
if errorlevel 1 goto failed
if not exist dist mkdir dist
copy /Y "target\release\sephiria-workshop-prep.exe" "dist\AveSepHelper.exe" >nul
copy /Y "README.md" "dist\README.md" >nul
echo Ready: dist\AveSepHelper.exe
goto end
:failed
echo Build failed. Install Rust and Visual Studio C++ Build Tools.
:end
pause
endlocal
