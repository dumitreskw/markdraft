@echo off
rem MarkDraft command-line launcher, installed with the app in <install folder>\bin.
rem   markdraft            open the current folder
rem   markdraft <path>     open a folder, Markdown file or workspace
rem   markdraft export ... headless PDF export (waits and prints to this window)
setlocal
set "APP=%~dp0..\markdraft.exe"
if "%~1"=="" (start "" "%APP%" . & exit /b 0)
for %%a in (export help -h --help -V --version) do if /i "%~1"=="%%a" ("%APP%" %* & exit /b)
start "" "%APP%" %*
