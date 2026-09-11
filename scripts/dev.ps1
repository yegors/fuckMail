param(
    [Parameter(Position=0)] [string] $Command = "doctor",
    [Parameter(ValueFromRemainingArguments=$true)] [string[]] $CommandArgs
)
$ErrorActionPreference = "Stop"
$taskRoot = Split-Path -Parent $PSScriptRoot

# Import the existing VS2022 compiler environment without changing global PATH.
function Enable-NativeCompiler {
    if (Get-Command cl.exe -ErrorAction SilentlyContinue) { return }
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path $vswhere)) {
        if ($Command -eq "bootstrap" -and (Get-Command winget -ErrorAction SilentlyContinue)) {
            & winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --silent --accept-package-agreements --accept-source-agreements --override '--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
            if ($LASTEXITCODE -ne 0) { throw "Visual Studio C++ Build Tools installation failed ($LASTEXITCODE)." }
        }
    }
    if (Test-Path $vswhere) {
        $install = & $vswhere -latest -version '[17.0,18.0)' -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($install) {
            $devShell = Join-Path $install 'Common7\Tools\Launch-VsDevShell.ps1'
            & $devShell -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
        }
    }
    if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
        throw "Install Visual Studio 2022 C++ Build Tools with the Windows SDK, then run scripts/dev.ps1 again. No existing compiler installation was replaced."
    }
}

if ($Command -in @('bootstrap','doctor','build','test','package','run')) { Enable-NativeCompiler }
$pythonCommand = Get-Command python.exe -ErrorAction SilentlyContinue
$pythonPath = if ($pythonCommand) { $pythonCommand.Source } else { $null }
if ($pythonPath -like '*WindowsApps*') { $pythonPath = $null }
if (-not $pythonPath) {
    if ($Command -eq 'bootstrap' -and (Get-Command winget -ErrorAction SilentlyContinue)) {
        & winget install --id Python.Python.3.11 --exact --silent --accept-package-agreements --accept-source-agreements
        if ($LASTEXITCODE -ne 0) { throw "Python developer-tool installation failed ($LASTEXITCODE)." }
        $pythonCandidate = Join-Path $env:LOCALAPPDATA 'Programs\Python\Python311\python.exe'
        if (Test-Path $pythonCandidate) { $pythonPath = $pythonCandidate }
    }
}
if (-not $pythonPath) { throw "Python 3.9+ is needed for the developer scripts. Install Python or run bootstrap with winget available." }
& $pythonPath "$taskRoot/scripts/dev.py" $Command @CommandArgs
exit $LASTEXITCODE
