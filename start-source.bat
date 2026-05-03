@echo off
cd /d "%~dp0"
echo AC Teleport - Source (auto-detect AC1 / AC EVO)
ac-teleport.exe source --unicast --target 192.168.50.2:5001 --bind 192.168.50.1:5001
pause
