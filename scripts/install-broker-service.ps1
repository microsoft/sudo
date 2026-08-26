# Install Sudo Elevation Broker Service
# This script installs and configures the SudoElevationBroker Windows service

[CmdletBinding()]
param(
    [Parameter()]
    [string]$ServicePath = "$PSScriptRoot\SudoElevationBroker.exe",
    
    [Parameter()]
    [switch]$Uninstall
)

$ServiceName = "SudoElevationBroker"
$DisplayName = "Sudo Elevation Broker"
$Description = "Provides elevation services for Sudo for Windows with Administrator Protection support"

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Install-BrokerService {
    Write-Host "Installing $DisplayName..." -ForegroundColor Cyan
    
    # Check if running as administrator
    if (-not (Test-Administrator)) {
        Write-Error "This script must be run as Administrator"
        exit 1
    }
    
    # Verify service executable exists
    if (-not (Test-Path $ServicePath)) {
        Write-Error "Service executable not found: $ServicePath"
        exit 1
    }
    
    # Stop existing service if it exists
    $existingService = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
    if ($existingService) {
        Write-Host "Stopping existing service..." -ForegroundColor Yellow
        Stop-Service -Name $ServiceName -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 2
        
        Write-Host "Removing existing service..." -ForegroundColor Yellow
        sc.exe delete $ServiceName | Out-Null
        Start-Sleep -Seconds 2
    }
    
    # Create service
    Write-Host "Creating service..." -ForegroundColor Green
    $result = sc.exe create $ServiceName binPath= $ServicePath start= auto DisplayName= $DisplayName
    
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to create service. Exit code: $LASTEXITCODE"
        exit 1
    }
    
    # Set service description
    sc.exe description $ServiceName $Description | Out-Null
    
    # Configure service recovery options (restart on failure)
    sc.exe failure $ServiceName reset= 86400 actions= restart/60000/restart/60000/restart/60000 | Out-Null
    
    # Start service
    Write-Host "Starting service..." -ForegroundColor Green
    Start-Service -Name $ServiceName
    
    # Verify service is running
    Start-Sleep -Seconds 2
    $service = Get-Service -Name $ServiceName
    
    if ($service.Status -eq 'Running') {
        Write-Host "`n✅ Service installed and started successfully!" -ForegroundColor Green
        Write-Host "   Service Name: $ServiceName" -ForegroundColor Gray
        Write-Host "   Display Name: $DisplayName" -ForegroundColor Gray
        Write-Host "   Status: Running" -ForegroundColor Gray
        Write-Host "   Start Type: Automatic" -ForegroundColor Gray
    } else {
        Write-Warning "Service installed but not running. Status: $($service.Status)"
        Write-Host "Check Event Viewer for errors: Application and Services Logs → $ServiceName" -ForegroundColor Yellow
    }
    
    # Create configuration directory
    $configDir = "$env:ProgramData\Microsoft\Sudo"
    if (-not (Test-Path $configDir)) {
        Write-Host "`nCreating configuration directory..." -ForegroundColor Cyan
        New-Item -ItemType Directory -Path $configDir -Force | Out-Null
        
        # Create default configuration file
        $configContent = @"
# Sudo Elevation Broker Service Configuration
# Generated: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")

[service]
log_level = "info"
max_concurrent_elevations = 10
default_timeout_ms = 30000

[security]
require_hello = true
allowed_commands = []  # Empty array = all commands allowed
audit_to_event_log = true

[named_pipe]
name = "\\\\.\pipe\\SudoElevationBroker"
max_message_size = 16777216  # 16MB
"@
        
        $configFile = Join-Path $configDir "config.toml"
        $configContent | Out-File -FilePath $configFile -Encoding UTF8
        Write-Host "   Configuration file: $configFile" -ForegroundColor Gray
    }
    
    Write-Host "`n📝 Next Steps:" -ForegroundColor Cyan
    Write-Host "   1. Enable Administrator Protection in Windows Security" -ForegroundColor White
    Write-Host "   2. Set up Windows Hello (Settings → Accounts → Sign-in options)" -ForegroundColor White
    Write-Host "   3. Test sudo: " -NoNewline -ForegroundColor White
    Write-Host "sudo whoami" -ForegroundColor Yellow
    Write-Host ""
}

function Uninstall-BrokerService {
    Write-Host "Uninstalling $DisplayName..." -ForegroundColor Cyan
    
    # Check if running as administrator
    if (-not (Test-Administrator)) {
        Write-Error "This script must be run as Administrator"
        exit 1
    }
    
    # Check if service exists
    $service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
    if (-not $service) {
        Write-Warning "Service '$ServiceName' not found. Nothing to uninstall."
        exit 0
    }
    
    # Stop service
    if ($service.Status -eq 'Running') {
        Write-Host "Stopping service..." -ForegroundColor Yellow
        Stop-Service -Name $ServiceName -Force
        Start-Sleep -Seconds 2
    }
    
    # Delete service
    Write-Host "Removing service..." -ForegroundColor Yellow
    sc.exe delete $ServiceName | Out-Null
    
    if ($LASTEXITCODE -eq 0) {
        Write-Host "✅ Service uninstalled successfully!" -ForegroundColor Green
    } else {
        Write-Error "Failed to uninstall service. Exit code: $LASTEXITCODE"
        exit 1
    }
    
    # Optionally remove configuration
    $configDir = "$env:ProgramData\Microsoft\Sudo"
    if (Test-Path $configDir) {
        Write-Host "`nConfiguration directory still exists: $configDir" -ForegroundColor Yellow
        $response = Read-Host "Remove configuration? (y/N)"
        if ($response -eq 'y' -or $response -eq 'Y') {
            Remove-Item -Path $configDir -Recurse -Force
            Write-Host "Configuration removed." -ForegroundColor Green
        }
    }
}

function Show-ServiceStatus {
    $service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
    
    if ($service) {
        Write-Host "`n📊 Service Status:" -ForegroundColor Cyan
        Write-Host "   Name: $($service.Name)" -ForegroundColor White
        Write-Host "   Display Name: $($service.DisplayName)" -ForegroundColor White
        Write-Host "   Status: $($service.Status)" -ForegroundColor $(if ($service.Status -eq 'Running') { 'Green' } else { 'Red' })
        Write-Host "   Start Type: $($service.StartType)" -ForegroundColor White
        
        # Check named pipe
        $pipePath = "\\.\pipe\SudoElevationBroker"
        $pipeExists = Test-Path $pipePath -ErrorAction SilentlyContinue
        Write-Host "   Named Pipe: $($if ($pipeExists) { 'Available ✓' } else { 'Not Found ✗' })" -ForegroundColor $(if ($pipeExists) { 'Green' } else { 'Red' })
        
        Write-Host ""
    } else {
        Write-Host "Service not installed." -ForegroundColor Yellow
    }
}

# Main execution
if ($Uninstall) {
    Uninstall-BrokerService
} else {
    Install-BrokerService
    Show-ServiceStatus
}
