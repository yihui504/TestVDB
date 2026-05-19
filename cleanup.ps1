param(
    [switch]$DeepClean,
    [switch]$ShrinkVhdx,
    [switch]$CleanRustCache,
    [switch]$Help
)

$ErrorActionPreference = "Continue"

if ($Help) {
    Write-Host @"
TestVDB Resource Cleanup Script
================================
Usage: .\cleanup.ps1 [options]

Options:
  -DeepClean      Full Docker cleanup (containers, networks, images, volumes, build cache)
  -ShrinkVhdx     Shrink WSL2/Docker vhdx disk images (requires Docker Desktop stop)
  -CleanRustCache Clean Rust build artifacts (target/ directories)
  -Help           Show this help message

Running without options performs all cleanup steps.
"@
    exit 0
}

if (-not $DeepClean -and -not $ShrinkVhdx -and -not $CleanRustCache) {
    $DeepClean = $true
    $ShrinkVhdx = $true
    $CleanRustCache = $true
}

$ProjectRoot = "C:\Users\11428\Desktop\mftui"

function Write-Step($msg) {
    Write-Host "`n=== $msg ===" -ForegroundColor Cyan
}

function Write-Ok($msg) {
    Write-Host "  [OK] $msg" -ForegroundColor Green
}

function Write-Warn($msg) {
    Write-Host "  [WARN] $msg" -ForegroundColor Yellow
}

if ($DeepClean) {
    Write-Step "Deep Docker Cleanup"

    $containers = docker ps -aq -f "name=testvdb-" 2>$null
    if ($containers) {
        $count = ($containers | Measure-Object).Count
        Write-Host "  Removing $count testvdb container(s)..."
        docker rm -f $containers 2>$null | Out-Null
        Write-Ok "Containers removed"
    } else {
        Write-Ok "No testvdb containers to remove"
    }

    $networks = docker network ls -q -f "name=testvdb-net-" 2>$null
    if ($networks) {
        $count = ($networks | Measure-Object).Count
        Write-Host "  Removing $count testvdb network(s)..."
        $networks | ForEach-Object { docker network rm $_ 2>$null | Out-Null }
        Write-Ok "Networks removed"
    } else {
        Write-Ok "No testvdb networks to remove"
    }

    Write-Host "  Pruning dangling Docker resources..."
    docker network prune -f 2>$null | Out-Null
    docker volume prune -f 2>$null | Out-Null
    docker image prune -f 2>$null | Out-Null
    docker builder prune -f 2>$null | Out-Null
    Write-Ok "Docker prune complete"

    $volumesDir = Join-Path $ProjectRoot "TestVDB\volumes"
    if (Test-Path $volumesDir) {
        Write-Host "  Cleaning volumes directory..."
        @("milvus", "qdrant", "etcd", "minio") | ForEach-Object {
            $target = Join-Path $volumesDir $_
            if (Test-Path $target) {
                Remove-Item $target -Recurse -Force -ErrorAction SilentlyContinue
                Write-Ok "Removed volumes/$_"
            }
        }
        New-Item -ItemType Directory -Path $volumesDir -Force | Out-Null
    }

    $dockerData = docker system df 2>$null
    Write-Host "`n  Docker disk usage after cleanup:"
    Write-Host $dockerData
}

if ($CleanRustCache) {
    Write-Step "Rust Build Cache Cleanup"

    $targetDirs = @(
        Join-Path $ProjectRoot "TestVDB\target",
        Join-Path $ProjectRoot "DeepSeek-TUI\target"
    )

    foreach ($dir in $targetDirs) {
        if (Test-Path $dir) {
            $sizeBefore = (Get-ChildItem $dir -Recurse -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
            $sizeBeforeMB = [math]::Round($sizeBefore / 1MB, 0)

            Write-Host "  Cleaning $dir ($sizeBeforeMB MB)..."
            Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
            Write-Ok "Removed $dir (freed ~$sizeBeforeMB MB)"
        } else {
            Write-Ok "$dir does not exist, skipping"
        }
    }

    $cargoRegistry = "$env:USERPROFILE\.cargo\registry"
    if (Test-Path $cargoRegistry) {
        $sizeBefore = (Get-ChildItem $cargoRegistry -Recurse -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
        $sizeBeforeMB = [math]::Round($sizeBefore / 1MB, 0)
        Write-Host "  Cargo registry size: $sizeBeforeMB MB (keeping - needed for rebuilds)"
    }
}

if ($ShrinkVhdx) {
    Write-Step "VHDX Disk Image Compression"

    $dockerRunning = Get-Process "Docker Desktop" -ErrorAction SilentlyContinue
    if ($dockerRunning) {
        Write-Warn "Docker Desktop is running. Stopping it for vhdx compression..."
        Stop-Process -Name "Docker Desktop" -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 5
    }

    wsl --shutdown 2>$null
    Start-Sleep -Seconds 3

    $vhdxFiles = @(
        @{ Name = "Project Docker Data"; Path = Join-Path $ProjectRoot "disk\docker_data.vhdx" },
        @{ Name = "Project WSL Ubuntu"; Path = Join-Path $ProjectRoot "main\ext4.vhdx" }
    )

    $wslBasePath = "$env:LOCALAPPDATA\Docker\wsl"
    if (Test-Path $wslBasePath) {
        Get-ChildItem $wslBasePath -Recurse -Filter "*.vhdx" -ErrorAction SilentlyContinue | ForEach-Object {
            $vhdxFiles += @{ Name = "Docker WSL Data"; Path = $_.FullName }
        }
    }

    $wslDataPath = "$env:LOCALAPPDATA\wsl"
    if (Test-Path $wslDataPath) {
        Get-ChildItem $wslDataPath -Recurse -Filter "*.vhdx" -ErrorAction SilentlyContinue | ForEach-Object {
            $vhdxFiles += @{ Name = "WSL Data"; Path = $_.FullName }
        }
    }

    foreach ($vhdx in $vhdxFiles) {
        if (Test-Path $vhdx.Path) {
            $file = Get-Item $vhdx.Path
            $sizeBeforeMB = [math]::Round($file.Length / 1MB, 0)
            Write-Host "  Compressing $($vhdx.Name) ($sizeBeforeMB MB)..."
            Write-Host "    Path: $($vhdx.Path)"

            $diskpartScript = @"
select vdisk file="$($vhdx.Path)"
attach vdisk readonly
compact vdisk
detach vdisk
exit
"@
            $tempScript = [System.IO.Path]::GetTempFileName()
            $diskpartScript | Set-Content $tempScript
            $result = diskpart /s $tempScript 2>&1
            Remove-Item $tempScript -Force -ErrorAction SilentlyContinue

            if (Test-Path $vhdx.Path) {
                $sizeAfterMB = [math]::Round((Get-Item $vhdx.Path).Length / 1MB, 0)
                $savedMB = $sizeBeforeMB - $sizeAfterMB
                if ($savedMB -gt 0) {
                    Write-Ok "Compressed: $sizeBeforeMB MB -> $sizeAfterMB MB (saved $savedMB MB)"
                } else {
                    Write-Ok "Already compact: $sizeAfterMB MB"
                }
            }
        } else {
            Write-Warn "$($vhdx.Name) not found at $($vhdx.Path)"
        }
    }

    Write-Host "`n  Restarting Docker Desktop..."
    $dockerExe = Get-ChildItem "C:\Program Files\Docker" -Filter "Docker Desktop.exe" -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($dockerExe) {
        Start-Process $dockerExe.FullName
        Write-Ok "Docker Desktop restarting"
    } else {
        Write-Warn "Could not find Docker Desktop executable. Please start it manually."
    }
}

Write-Step "Cleanup Summary"
$os = Get-CimInstance Win32_OperatingSystem
$totalGB = [math]::Round($os.TotalVisibleMemorySize / 1MB, 1)
$freeGB = [math]::Round($os.FreePhysicalMemory / 1MB, 1)
Write-Host "  System Memory: $freeGB GB free / $totalGB GB total"
Write-Host "  Docker Status:"
docker system df 2>$null | ForEach-Object { Write-Host "    $_" }
Write-Host "`nDone!" -ForegroundColor Green
