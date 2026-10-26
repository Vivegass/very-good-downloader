# Расширение для Firefox: из папки extension\ делает подписанный Mozilla .xpi.
#   powershell -ExecutionPolicy Bypass -File rust\firefox.ps1
# Ключи addons.mozilla.org (API key / secret) — в %USERPROFILE%\.vgd\amo.json: {"issuer": "...", "secret": "..."}
# или в переменных WEB_EXT_API_KEY / WEB_EXT_API_SECRET. Подпись — «unlisted» (без публикации в каталоге).
# Mozilla подписывает каждую версию один раз: готовый .xpi хранится в rust\_dl\firefox\ и берётся оттуда.
# Результат: rust\_dl\firefox\very-good-downloader-<версия>.xpi (путь печатается последней строкой).
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$root = Split-Path $here
$src = Join-Path $root 'extension'
$work = Join-Path $here '_dl\firefox'
New-Item -ItemType Directory -Force $work | Out-Null

$manifest = Get-Content (Join-Path $src 'manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
$version = $manifest.version
$xpi = Join-Path $work "very-good-downloader-$version.xpi"
if (Test-Path $xpi) {
    Write-Host "Firefox: уже подписано — $xpi"
    return $xpi
}

if (-not $env:WEB_EXT_API_KEY) {
    $keys = Join-Path $env:USERPROFILE '.vgd\amo.json'
    if (-not (Test-Path $keys)) { throw "нет ключей addons.mozilla.org: $keys" }
    $k = Get-Content $keys -Raw | ConvertFrom-Json
    $env:WEB_EXT_API_KEY = $k.issuer
    $env:WEB_EXT_API_SECRET = $k.secret
}

# Копия расширения с манифестом для Firefox: фоновый скрипт вместо service worker и id дополнения.
$stage = Join-Path $work 'src'
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
Copy-Item -Recurse $src $stage
$manifest.background = [pscustomobject]@{ scripts = @('background.js') }
$gecko = [pscustomobject]@{
    id = 'very-good-downloader@vivegass.github.io'
    # world: MAIN у content_scripts — с Firefox 128
    strict_min_version = '128.0'
    data_collection_permissions = [pscustomobject]@{ required = @('none') }
}
$manifest | Add-Member -NotePropertyName browser_specific_settings -NotePropertyValue ([pscustomobject]@{ gecko = $gecko }) -Force
$json = $manifest | ConvertTo-Json -Depth 20
[IO.File]::WriteAllText((Join-Path $stage 'manifest.json'), $json, (New-Object Text.UTF8Encoding $false))

$out = Join-Path $work 'signed'
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
$env:Path = "$env:ProgramFiles\nodejs;$env:Path"
$webext = Join-Path $here '_dl\webext\node_modules\.bin\web-ext.cmd'
if (-not (Test-Path $webext)) {
    & npm install --prefix (Join-Path $here '_dl\webext') --no-audit --no-fund --loglevel=error web-ext@8
}
& $webext sign --source-dir $stage --artifacts-dir $out --channel unlisted --timeout 900000
if ($LASTEXITCODE) { throw 'web-ext sign завершился с ошибкой' }
$signed = Get-ChildItem $out -Filter *.xpi | Select-Object -First 1
if (-not $signed) { throw 'подписанный .xpi не появился' }
Move-Item $signed.FullName $xpi
Write-Host "Firefox: подписано — $xpi"
return $xpi
