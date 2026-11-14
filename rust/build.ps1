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
    # Firefox: подписанный Mozilla .xpi (rust\firefox.ps1; без ключей addons.mozilla.org — пропускается).
    try {
        $xpi = & (Join-Path $here 'firefox.ps1') | Select-Object -Last 1
        if ($xpi -and (Test-Path $xpi)) { Copy-Item $xpi (Join-Path $dist 'extension-firefox.xpi') }
    } catch { Write-Host "Firefox: расширение не собрано — $_" }

    # yt-dlp — всегда свежий с GitHub (YouTube часто ломает старые версии).
    New-Item -ItemType Directory -Force '_dl' | Out-Null
    Invoke-WebRequest -UseBasicParsing 'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe' -OutFile '_dl\yt-dlp.exe'
    Copy-Item '_dl\yt-dlp.exe' $bin

    # ffmpeg (FFmpeg Essentials; ffprobe не нужен — сведения о файле берутся из ffmpeg).
    $ff = Get-ChildItem '_dl\ff' -Recurse -Filter ffmpeg.exe -ErrorAction SilentlyContinue | Select-Object -First 1
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

    # Portable: та же папка в zip (внутри — VeryGoodDownloader\…; имена в UTF-8).
    $release = Join-Path $root 'release'
    New-Item -ItemType Directory -Force $release | Out-Null
    $zip = Join-Path $release "VeryGoodDownloader-$version-portable.zip"
    Remove-Item -Force $zip -ErrorAction SilentlyContinue
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($dist, $zip, [IO.Compression.CompressionLevel]::Optimal, $true)

    # SHA-256 рядом с каждым ассетом: «<хэш>  <имя>». Программа сверяет скачанное обновление
    # с полем digest из GitHub API, а если его нет — с этим файлом (без суммы не обновляется).
    $assets = @("VeryGoodDownloader-Setup-$version.exe", "VeryGoodDownloader-$version-portable.zip") |
        ForEach-Object { Join-Path $release $_ } | Where-Object { Test-Path $_ }
    foreach ($a in $assets) {
        $h = (Get-FileHash -Algorithm SHA256 -LiteralPath $a).Hash.ToLower()
        [IO.File]::WriteAllText("$a.sha256", "$h  $(Split-Path $a -Leaf)`n")
    }
    # Заметка: что приложить к релизу на GitHub.
    $list = ($assets | ForEach-Object { $n = Split-Path $_ -Leaf; "  $n`n  $n.sha256" }) -join "`n"
    $note = @"
Релиз Very Good Downloader $version для Windows

Тег: v$version  (Android — отдельные релизы с тегом android-v<версия>, см. rust\README.md → «Обновления»)
Не отмечать «Set as a pre-release», если это не бета: программа предлагает пре-релизы только с галочкой «бета-версии».

Приложить к релизу (Assets):
$list

Имена файлов не менять: программа ищет ассет по имени (VeryGoodDownloader-Setup-<версия>.exe,
VeryGoodDownloader-<версия>-portable.zip). Описание релиза (markdown) показывается в окне по «что нового».
"@
    [IO.File]::WriteAllText((Join-Path $release "РЕЛИЗ-$version.txt"), $note, (New-Object Text.UTF8Encoding $true))
    Write-Host "Ассеты и .sha256 — в $release, что приложить к релизу — РЕЛИЗ-$version.txt"
}
finally { Pop-Location }
Get-ChildItem (Join-Path $here 'dist\VeryGoodDownloader'), (Join-Path $here 'dist\VeryGoodDownloader\bin') -File |
    Select-Object Name, @{n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } }
