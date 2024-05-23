@echo off

net session >nul 2>&1
if %errorLevel% == 0 (
    goto :do_it
)

echo You need to be admin to enable sudo!
goto :exit

:do_it
echo Enabling sudo...

set key=HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Sudo
set value=Enabled
set data=3
reg add "%key%" /v "%value%" /t REG_DWORD /d %data% /f

:exit
