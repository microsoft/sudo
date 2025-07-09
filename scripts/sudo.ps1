<#
	.SYNOPSIS
		Runs a scriptblock, command or application as an elevated process using sudo.exe
	.DESCRIPTION
		Wraps sudo.exe to add functionality for running PowerShell scripts or commands in an elevated process.

		When running a scriptblock or PowerShell command, a new copy of PowerShell is run with sudo and an EncodedCommand.
		When running a native application, the command itself is run with sudo.

		This script DOES NOT (currently) support piping input to the elevated command.
#>

# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
[CmdletBinding(DefaultParameterSetName = "Script")]
param(
	# A scriptblock to run in an elevated process
	[Parameter(Mandatory, Position = 0, ParameterSetName = "Script")]
	[scriptblock]$ScriptBlock,

	# Run PowerShell with the -NoProfile switch
	[switch]$NoProfile,

	# A command or application to run in an elevated process
	[Parameter(Mandatory, Position = 0, ParameterSetName = "Command")]
	[string]$Command,

	# Arguments to pass to the command or application
	[Parameter(Position = 1, ParameterSetName = "Command", ValueFromRemainingArguments)]
	[Alias("Args")]
	[PSObject[]]$ArgumentList
)
begin {
	if ($IsLinux -or $IsMacOS) {
		throw "This script works only on Microsoft Windows"
	}

	if ($__SUDO_TEST -ne $true) {
		$Env:SUDOEXE = "sudo.exe"
	} elseif (!$Env:SUDOEXE) {
		throw "Environment variable SUDOEXE has not been set for testing"
	}

	if (!(Get-Command -Type Application -Name $Env:SUDOEXE -ErrorAction Ignore)) {
		throw "Env:SUDOEXE is set to '$Env:SUDOEXE' but it cannot be found."
	}

	$thisPowerShell = (Get-Process -Id $PID).MainModule.FileName
	if (!$thisPowerShell) {
		throw "Cannot determine PowerShell executable path."
	}

	function ConvertToBase64EncodedString([string]$InputObject) {
		$bytes = [System.Text.Encoding]::Unicode.GetBytes($InputObject)
		[Convert]::ToBase64String($bytes)
	}
}

end {
	# If the first parameter is the name of an executable, just run that without PowerShell
	if ($PSCmdlet.ParameterSetName -eq "Command") {
		if (@(Get-Command $Command -ErrorAction Ignore)[0].CommandType -eq "Application") {
			# NOTE: this assumes that all the parameters can be just strings
			if ($PSBoundParameters.Contains("Debug")) {
				Trace-Command -PSHost -Name param* -Expression { & $Env:SUDOEXE $Command @ArgumentList }
			} else {
				& $Env:SUDOEXE $Command $ArgumentList
			}
			return
		} else {
			# In this case, we're going to need to _make_ a scriptblock out of $MyInvocation.Statement
			# NOT $MyInvocation.Line because there might be more than one line in the statement
			# IISReset and Jaykul apologize for the reflection, but we need to support old versions of PowerShell
			$Statement = [System.Management.Automation.InvocationInfo].GetMember(
					'_scriptPosition',
					[System.Reflection.BindingFlags]'NonPublic,Instance'
				)[0].GetValue($MyInvocation).Text.
			# Strip the 'sudo' or 'sudo.ps1` or whatever off the front of the statement
			$Statement = $Statement.SubString($MyInvocation.InvocationName.Length).Trim()
			$EncodedCommand = ConvertToBase64EncodedString $Statement
		}
	} else {
		$EncodedCommand = ConvertToBase64EncodedString $scriptBlock
	}

	$switches = @("-NoLogo", "-NonInteractive")
	if ($NoProfile)	{ $switches += "-NoProfile" }

	if ($PSBoundParameters.Contains("Debug")) {
		Trace-Command -PSHost -Name param* -Expression { & $Env:SUDOEXE $ThisPowerShell @switches -EncodedCommand $encodedCommand }
	} else {
		& $Env:SUDOEXE $ThisPowerShell @switches -EncodedCommand $encodedCommand
	}
}
