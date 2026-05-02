@echo off
cd /d "%~dp0"
sim-relay.exe source --target 192.168.50.2 --games pcars2
pause
