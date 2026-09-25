$ErrorActionPreference = "Stop"

$Repository = "purefunctor/purescript-iris"
$Binary = "iris.exe"
$InstallDirectory = if ($env:IRIS_INSTALL_DIR) {
    $env:IRIS_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA "Iris\bin"
}
$Version = if ($env:IRIS_VERSION) { $env:IRIS_VERSION } else { "latest" }

if (-not [Environment]::Is64BitOperatingSystem) {
    throw "Iris supports only 64-bit Windows"
}

if ($Version -eq "latest") {
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository/releases/latest"
    $Version = $Release.tag_name
}

if ($Version -notmatch '^v[0-9]') {
    throw "Invalid release version: $Version"
}

if ($Version -match '^v0\.0\.') {
    throw "This installer requires v0.1.0 or later; use the installer from the requested release tag for v0.0.x"
}

$Target = "x86_64-pc-windows-msvc"
$ArchiveName = "iris-$Target.zip"
$ArchiveUrl = "https://github.com/$Repository/releases/download/$Version/$ArchiveName"
$TemporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("iris-install-" + [guid]::NewGuid())
$Archive = Join-Path $TemporaryDirectory $ArchiveName

New-Item -ItemType Directory -Path $TemporaryDirectory | Out-Null
try {
    Write-Host "Downloading iris $Version for $Target"
    Invoke-WebRequest -Uri $ArchiveUrl -OutFile $Archive

    if ($env:IRIS_SKIP_ATTESTATION -eq "1") {
        Write-Warning "GitHub release attestation verification skipped (IRIS_SKIP_ATTESTATION=1)."
    } else {
        $GitHubAttestationsAvailable = if (Get-Command gh -ErrorAction SilentlyContinue) {
            & gh attestation verify --help 2>$null | Out-Null
            $LASTEXITCODE -eq 0
        } else {
            $false
        }

        if ($GitHubAttestationsAvailable) {
            Write-Host "Verifying GitHub release attestation"
            & gh attestation verify $Archive --repo $Repository | Out-Null
            if ($LASTEXITCODE -ne 0) {
                throw "GitHub release attestation verification failed"
            }
        } else {
            Write-Warning "A GitHub CLI with attestation support is not installed; release provenance was not verified."
            Write-Warning "Install or update gh from https://cli.github.com/ to verify future installations."
        }
    }

    Expand-Archive -LiteralPath $Archive -DestinationPath $TemporaryDirectory
    $ExtractedBinary = Join-Path $TemporaryDirectory "iris-$Target\$Binary"
    if (-not (Test-Path -LiteralPath $ExtractedBinary -PathType Leaf)) {
        throw "Release archive does not contain $Binary"
    }

    New-Item -ItemType Directory -Force -Path $InstallDirectory | Out-Null
    $Destination = Join-Path $InstallDirectory $Binary
    $InstallationFile = Join-Path $InstallDirectory (".iris-install-" + [guid]::NewGuid() + ".exe")
    Copy-Item -LiteralPath $ExtractedBinary -Destination $InstallationFile
    Move-Item -Force -LiteralPath $InstallationFile -Destination $Destination

    Write-Host "Installed $Version to $Destination"
    if ($env:PATH.Split([IO.Path]::PathSeparator) -notcontains $InstallDirectory) {
        Write-Host "Add $InstallDirectory to PATH to run iris."
    }
} finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $TemporaryDirectory
}
