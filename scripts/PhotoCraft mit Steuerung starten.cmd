@echo off
rem Startet PhotoCraft mit eingeschalteter Steuerschnittstelle, damit
rem unterwasser-analyse.exe den Filter direkt anwenden kann.
rem Port und Token-Datei muessen zu den Feldern unter "PhotoCraft-Verbindung" passen.
if not exist "%LOCALAPPDATA%\uwcc" mkdir "%LOCALAPPDATA%\uwcc"
start "" "%ProgramFiles%\PhotoCraft\photocraft.exe" --control 7878 --control-token-file "%LOCALAPPDATA%\uwcc\photocraft-control.token" %*
