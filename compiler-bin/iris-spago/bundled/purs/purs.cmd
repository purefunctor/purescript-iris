@echo off
if "%~1"=="--version" if "%~2"=="" (
  echo 0.15.15
  exit /b 0
)
echo Iris compatibility shim only supports purs --version 1>&2
exit /b 64
