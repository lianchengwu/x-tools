# Windows 单实例冒烟测试：验证 xtools 宿主（悬浮球+托盘）在 Windows 上的
# 单实例互斥、二次拉起合并退出、正常退出后可再次启动。
# 用法: ./scripts/windows-singleton-smoke.ps1 -TargetDir <release 目录>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $TargetDir,
    [int] $TimeoutSeconds = 15
)

$ErrorActionPreference = 'Stop'
if (-not $env:SLINT_BACKEND) {
    $env:SLINT_BACKEND = 'winit-software'
}

if (-not [Environment]::UserInteractive) {
    throw 'Windows singleton smoke test requires an interactive desktop session (the GitHub Windows runner provides one).'
}

$tools = @('xtools')
$started = [System.Collections.Generic.List[System.Diagnostics.Process]]::new()
$bodyError = $null

function Wait-ForExit([System.Diagnostics.Process] $Process, [int] $Milliseconds) {
    if (-not $Process.WaitForExit($Milliseconds)) {
        $Process.Refresh()
        throw "Process $($Process.Id) did not exit within $Milliseconds ms."
    }
}

function Assert-Alive([System.Diagnostics.Process] $Process, [string] $Description, [string] $ErrPath = $null, [string] $OutPath = $null) {
    $Process.Refresh()
    if ($Process.HasExited) {
        $extra = ""
        if ($ErrPath -and (Test-Path -LiteralPath $ErrPath)) {
            $errContent = Get-Content -Raw -LiteralPath $ErrPath
            if ($errContent) { $extra += "`nProcess Stderr:`n$errContent" }
        }
        if ($OutPath -and (Test-Path -LiteralPath $OutPath)) {
            $outContent = Get-Content -Raw -LiteralPath $OutPath
            if ($errContent) { $extra += "`nProcess Stdout:`n$outContent" }
        }
        throw "$Description process $($Process.Id) exited unexpectedly (exit code $($Process.ExitCode)).$extra"
    }
}
function Stop-SingletonTool([string] $ToolName, [System.Diagnostics.Process] $Process) {
    try {
        $pipeName = "xtools-$env:USERNAME-$ToolName"
        $pipe = [System.IO.Pipes.NamedPipeClientStream]::new('.', $pipeName, [System.IO.Pipes.PipeDirection]::Out)
        $pipe.Connect(500)
        $writer = [System.IO.StreamWriter]::new($pipe)
        $writer.WriteLine("QUIT")
        $writer.Flush()
        $pipe.Dispose()
    } catch {
        # Ignore connect error and fallback to window close / process kill
    }

    $Process.CloseMainWindow() | Out-Null
    if (-not $Process.WaitForExit(3000)) {
        $Process.Kill()
        [void]$Process.WaitForExit(3000)
    }
    if (-not $Process.HasExited) {
        throw "Could not terminate first $ToolName process $($Process.Id)."
    }
}


try {
    foreach ($tool in $tools) {
        $path = Join-Path $TargetDir "$tool.exe"
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Built tool not found: $path"
        }

        Write-Host "Testing singleton behavior for $tool"
        $firstErr = Join-Path $env:TEMP "$tool-first-stderr-$([guid]::NewGuid()).log"
        $firstOut = Join-Path $env:TEMP "$tool-first-stdout-$([guid]::NewGuid()).log"
        $first = Start-Process -FilePath $path -WorkingDirectory $TargetDir -RedirectStandardError $firstErr -RedirectStandardOutput $firstOut -PassThru
        $started.Add($first)
        Start-Sleep -Milliseconds 800
        Assert-Alive $first 'First' $firstErr $firstOut

        $secondErr = Join-Path $env:TEMP "$tool-second-stderr-$([guid]::NewGuid()).log"
        $secondOut = Join-Path $env:TEMP "$tool-second-stdout-$([guid]::NewGuid()).log"
        $second = Start-Process -FilePath $path -WorkingDirectory $TargetDir -RedirectStandardError $secondErr -RedirectStandardOutput $secondOut -PassThru
        $started.Add($second)
        Wait-ForExit $second ($TimeoutSeconds * 1000)
        if ($second.ExitCode -ne 0) {
            $errContent = if (Test-Path -LiteralPath $secondErr) { Get-Content -Raw -LiteralPath $secondErr } else { "" }
            throw "Second $tool launch exited with code $($second.ExitCode), expected a normal singleton handoff.`nStderr: $errContent"
        }
        Assert-Alive $first 'First' $firstErr $firstOut

        Stop-SingletonTool $tool $first
        Start-Sleep -Milliseconds 500

        $thirdErr = Join-Path $env:TEMP "$tool-third-stderr-$([guid]::NewGuid()).log"
        $thirdOut = Join-Path $env:TEMP "$tool-third-stdout-$([guid]::NewGuid()).log"
        $third = Start-Process -FilePath $path -WorkingDirectory $TargetDir -RedirectStandardError $thirdErr -RedirectStandardOutput $thirdOut -PassThru
        $started.Add($third)
        Start-Sleep -Milliseconds 800
        Assert-Alive $third 'Later launch' $thirdErr $thirdOut
        Write-Host "$tool singleton smoke test passed"
    }
}
catch {
    $bodyError = $_
}
finally {
    $cleanupErrors = [System.Collections.Generic.List[System.Exception]]::new()
    foreach ($process in $started) {
        try {
            $process.Refresh()
            if (-not $process.HasExited) {
                $process.Kill()
                Wait-ForExit $process 3000
            }
        } catch {
            $cleanupErrors.Add([System.Exception]::new(
                "Cleanup failed for process $($Process.Id): $($_.Exception.Message)",
                $_.Exception
            ))
        }
    }

    if ($cleanupErrors.Count -gt 0) {
        if ($null -ne $bodyError) {
            $cleanupErrors.Insert(0, $bodyError.Exception)
        }
        throw [System.AggregateException]::new('Windows singleton smoke test or process cleanup failed.', $cleanupErrors)
    }
}

if ($null -ne $bodyError) {
    throw $bodyError
}
