@echo off
cd /d "%~dp0"
echo AC Teleport - Target (auto-detect AC1 / AC EVO)
ac-teleport.exe target --unicast --bind 192.168.50.2:5001
pause
