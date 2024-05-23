# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# open question - Should -NoProfile be used when invoking PowerShell
BEGIN {
    if ($__SUDO_TEST -ne $true) {
        [System.String]$SUDOEXE = 'sudo.exe'
    }
    else {
        if ($null -eq $SUDOEXE) {
            throw [System.InvalidOperationException] 'Variable SUDOEXE has not been set for testing.'
        }
    }

    if ([Environment]::OSVersion.Platform -ne 'Win32NT') {
        throw [System.PlatformNotSupportedException] 'This script works only on Microsoft Windows'
    }

    if ($null -eq (Get-Command -Type Application -Name "$SUDOEXE" -ErrorAction Ignore)) {
        throw [System.IO.FileNotFoundException] "'$SUDOEXE' cannot be found."
    }

    $psProcess = Get-Process -Id $PID
    if (($null -eq $psProcess) -or ($psProcess.Count -ne 1)) {
        throw [System.InvalidOperationException] "Cannot retrieve process for '$PID'"
    }

    $thisPowerShell = $psProcess.MainModule.FileName
    if ($null -eq $thisPowerShell) {
        throw [System.InvalidOperationException] "Cannot determine path to '$psProcess'"
    }

    function ConvertTo-Base64EncodedString([System.String]$cmdLine) {
        $bytes = [System.Text.Encoding]::Unicode.GetBytes($cmdLine)
        [Convert]::ToBase64String($bytes)
    }

    $MI = $MyInvocation
}

END {
    $cmdArguments = $args

    # short-circuit if the user provided a scriptblock, then we will use it and ignore any other arguments
    if ($cmdArguments.Count -eq 1 -and $cmdArguments[0] -is [scriptblock]) {
        $scriptBlock = $cmdArguments[0]
        $encodedCommand = ConvertTo-Base64EncodedString -cmdLine ($scriptBlock.ToString())
        if (($psversiontable.psversion.major -eq 7) -and ($__SUDO_DEBUG -eq $true)) {
            Trace-Command -PSHost -Name param* -Expression { & $SUDOEXE "$thisPowerShell" -e $encodedCommand }
        }
        else {
            & $SUDOEXE "$thisPowerShell" -e $encodedCommand
        }
        return
    }

    $cmdLine = $MI.Line
    $sudoOffset = $cmdLine.IndexOf($MI.InvocationName)
    $cmdLineWithoutScript = $cmdLine.SubString($sudoOffset + 5)
    $cmdLineAst = [System.Management.Automation.Language.Parser]::ParseInput($cmdLineWithoutScript, [ref]$null, [ref]$null)
    $commandAst = $cmdLineAst.Find({ $args[0] -is [System.Management.Automation.Language.CommandAst] }, $false)
    $commandName = $commandAst.GetCommandName()
    $isApplication = Get-Command -Type Application -Name $commandName -ErrorAction Ignore | Select-Object -First 1
    $isCmdletOrScript = Get-Command -Type Cmdlet, ExternalScript -Name $commandName -ErrorAction Ignore | Select-Object -First 1

    # if the command is a native command, just invoke it
    if ($null -ne $isApplication) {
        if (($psversiontable.psversion.major -eq 7) -and ($__SUDO_DEBUG -eq $true)) {
            Trace-Command -PSHost -Name param* -Expression { & $SUDOEXE $cmdArguments }
        }
        else {
            & $SUDOEXE $cmdArguments
        }
    }
    elseif ($null -ne $isCmdletOrScript) {
        $encodedCommand = ConvertTo-Base64EncodedString($cmdLineWithoutScript)
        if (($psversiontable.psversion.major -eq 7) -and ($__SUDO_DEBUG -eq $true)) {
            Trace-Command -PSHost -Name param* -Expression { & $SUDOEXE -nologo -e $encodedCommand }
        }
        else {
            & $SUDOEXE $thisPowerShell -nologo -e $encodedCommand
        }
    }
    else {
        throw [System.Management.Automation.CommandNotFoundException] "Cannot find '$commandName'"
    }
}

