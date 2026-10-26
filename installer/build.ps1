# Сборка установщика: .\installer\build.ps1
# Результат: release\VeryGoodDownloader-Setup-<версия>.exe
$ErrorActionPreference = 'Continue'
$here = $PSScriptRoot
$root = Split-Path $here
$py = Join-Path $here '.buildenv\Scripts\python.exe'

if (-not (Test-Path $py)) {
    & "$env:LOCALAPPDATA\Programs\Python\Python312\python.exe" -m venv (Join-Path $here '.buildenv')
}
& $py -m pip install -q --upgrade 'yt-dlp[default,curl-cffi]' pyinstaller pillow
$version = (Select-String -Path (Join-Path $root 'appinfo.py') -Pattern 'APP_VERSION = "(.+)"').Matches[0].Groups[1].Value

& $py -I (Join-Path $here 'make_icon.py') (Join-Path $root 'app.ico')
Push-Location $here
try {
    & $py -m PyInstaller --noconfirm --clean --distpath dist --workpath build vgd.spec
    if ($LASTEXITCODE) { throw 'PyInstaller завершился с ошибкой' }

    # ffmpeg/ffprobe (FFmpeg Essentials) и deno — рядом с программой, в bin\
    $bin = 'dist\VeryGoodDownloader\bin'
    New-Item -ItemType Directory -Force $bin | Out-Null
    $ff = Get-ChildItem '_dl\ff' -Recurse -Filter ffmpeg.exe | Select-Object -First 1
    if (-not $ff) {
        winget download -e --id Gyan.FFmpeg.Essentials -d _dl --accept-package-agreements --accept-source-agreements
        Expand-Archive -Force (Get-ChildItem '_dl\*.zip').FullName '_dl\ff'
        $ff = Get-ChildItem '_dl\ff' -Recurse -Filter ffmpeg.exe | Select-Object -First 1
    }
    Copy-Item (Join-Path $ff.DirectoryName 'ffmpeg.exe'), (Join-Path $ff.DirectoryName 'ffprobe.exe') $bin
    $deno = (Get-Command deno -ErrorAction SilentlyContinue).Source
    if (-not $deno) {
        $deno = (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter deno.exe | Select-Object -First 1).FullName
    }
    Copy-Item $deno $bin
    & $py -c 'import yt_dlp.version as v; print(v.__version__)' |
        Set-Content -Encoding ascii 'dist\VeryGoodDownloader\yt_dlp_version.txt'

    $iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe") |
        Where-Object { Test-Path $_ } | Select-Object -First 1
    & $iscc "/DAppVersion=$version" VeryGoodDownloader.iss
    if ($LASTEXITCODE) { throw 'Inno Setup завершился с ошибкой' }
}
finally { Pop-Location }
Get-ChildItem (Join-Path $root 'release') | Select-Object Name, @{n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } }
