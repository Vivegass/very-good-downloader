# Сборка Rust-версии: powershell -ExecutionPolicy Bypass -File rust\build.ps1
# Результат: rust\dist\VeryGoodDownloader\ (готовая папка) и, если есть Inno Setup,
# release\VeryGoodDownloader-Setup-<версия>.exe
$ErrorActionPreference = 'Continue'
$here = $PSScriptRoot
$root = Split-Path $here
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path $cargo)) { $cargo = 'cargo' }

$version = (Select-String -Path (Join-Path $here 'Cargo.toml') -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
if (Get-Process -Name 'Very Good Downloader' -ErrorAction SilentlyContinue) { throw 'Закройте Very Good Downloader перед сборкой: открытый exe нельзя заменить' }
Push-Location $here
try {
    & $cargo build --release
    if ($LASTEXITCODE) { throw 'cargo build завершился с ошибкой' }

    $dist = Join-Path $here 'dist\VeryGoodDownloader'
    Remove-Item -Recurse -Force $dist -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force (Join-Path $dist 'bin') | Out-Null
    Copy-Item 'target\release\vgd-gui.exe' (Join-Path $dist 'Very Good Downloader.exe')
    Copy-Item 'target\release\vgd.exe' (Join-Path $dist 'vgd.exe')
    $bin = Join-Path $dist 'bin'
    # Расширение для Chrome / Edge / Яндекс.Браузера — ставится «Загрузить распакованное».
    Copy-Item -Recurse (Join-Path $root 'extension') (Join-Path $dist 'extension')
    Copy-Item (Join-Path $here 'installer\Как установить.txt') $dist

    # yt-dlp — всегда свежий с GitHub (YouTube часто ломает старые версии).
    New-Item -ItemType Directory -Force '_dl' | Out-Null
    Invoke-WebRequest -UseBasicParsing 'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe' -OutFile '_dl\yt-dlp.exe'
    Copy-Item '_dl\yt-dlp.exe' $bin

    # ffmpeg (FFmpeg Essentials; ffprobe не нужен — сведения о файле берутся из ffmpeg).
    $ff = Get-ChildItem (Join-Path $root 'installer\_dl\ff'), '_dl\ff' -Recurse -Filter ffmpeg.exe -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $ff) {
        Invoke-WebRequest -UseBasicParsing 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip' -OutFile '_dl\ff.zip'
        Expand-Archive -Force '_dl\ff.zip' '_dl\ff'
        $ff = Get-ChildItem '_dl\ff' -Recurse -Filter ffmpeg.exe | Select-Object -First 1
    }
    Copy-Item $ff.FullName $bin

    # deno — JS-движок, без него yt-dlp получает не все форматы YouTube.
    $deno = @((Join-Path $here 'bin\deno.exe'), (Get-Command deno -ErrorAction SilentlyContinue).Source) |
        Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $deno) {
        Invoke-WebRequest -UseBasicParsing 'https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip' -OutFile '_dl\deno.zip'
        Expand-Archive -Force '_dl\deno.zip' '_dl\deno'
        $deno = '_dl\deno\deno.exe'
    }
    Copy-Item $deno $bin

    $iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe") |
        Where-Object { Test-Path $_ } | Select-Object -First 1
    if ($iscc) {
        & $iscc "/DAppVersion=$version" 'installer\VeryGoodDownloader.iss'
        if ($LASTEXITCODE) { throw 'Inno Setup завершился с ошибкой' }
    } else {
        Write-Host 'Inno Setup не найден — установщик не собран, готовая папка: rust\dist\VeryGoodDownloader'
    }
}
finally { Pop-Location }
Get-ChildItem (Join-Path $here 'dist\VeryGoodDownloader'), (Join-Path $here 'dist\VeryGoodDownloader\bin') -File |
    Select-Object Name, @{n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } }
