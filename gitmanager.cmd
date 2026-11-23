@echo off
rem Lance GITManager sous Windows, meme sur un ordinateur sans Python.
rem Utilise uv s'il est present, sinon Python 3.9 ou plus. S'il n'y a ni l'un
rem ni l'autre, installe uv avec winget, qui verifie l'empreinte du fichier
rem telecharge ; uv fournit ensuite Python. Le programme installe lui-meme
rem Git et GitHub CLI.
setlocal
set "SCRIPT=%~dp0gitmanager.py"

call :trouver_uv
if defined UV goto lancer_uv
call :trouver_python
if defined PY goto lancer_python

where winget >nul 2>nul
if errorlevel 1 goto sans_winget
echo GITManager a besoin de Python : installation de uv, qui le fournit.
winget install --id astral-sh.uv -e --source winget --accept-source-agreements --accept-package-agreements
call :trouver_uv
if defined UV goto lancer_uv
echo uv est installe mais pas encore visible : ferme cette fenetre et relance gitmanager.cmd.
pause
exit /b 1

:sans_winget
echo winget est introuvable : installe Python depuis https://www.python.org/downloads/ puis relance.
pause
exit /b 1

:lancer_uv
"%UV%" run "%SCRIPT%"
if errorlevel 1 pause
exit /b

:lancer_python
%PY% "%SCRIPT%"
if errorlevel 1 pause
exit /b

:trouver_uv
set "UV="
for %%U in (uv.exe) do if not "%%~$PATH:U"=="" set "UV=%%~$PATH:U"
if not defined UV if exist "%USERPROFILE%\.local\bin\uv.exe" set "UV=%USERPROFILE%\.local\bin\uv.exe"
if not defined UV if exist "%LOCALAPPDATA%\Microsoft\WinGet\Links\uv.exe" set "UV=%LOCALAPPDATA%\Microsoft\WinGet\Links\uv.exe"
if not defined UV for /d %%D in ("%LOCALAPPDATA%\Microsoft\WinGet\Packages\astral-sh.uv_*") do if exist "%%D\uv.exe" set "UV=%%D\uv.exe"
exit /b 0

:trouver_python
set "PY="
py -3 -c "import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)" >nul 2>nul && set "PY=py -3"
if not defined PY python -c "import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)" >nul 2>nul && set "PY=python"
exit /b 0
