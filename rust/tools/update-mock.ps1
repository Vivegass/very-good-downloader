# Mock GitHub Releases для проверки обновления без настоящего релиза (только для отладки).
#
#   powershell -ExecutionPolicy Bypass -File rust\tools\update-mock.ps1 -Assets <папка> -Version 1.11.1 [-NoDigest] [-CutOnce] [-Prerelease]
#
# Отдаёт http://localhost:<порт>/releases — список из одного релиза v<версия> в формате GitHub API
# со всеми файлами папки как ассетами (размер, digest "sha256:…"), и сами файлы с поддержкой Range (докачка).
#   -NoDigest  — без поля digest: программа должна взять хэш из ассета <имя>.sha256
#   -CutOnce   — первую загрузку каждого файла оборвать на середине (проверка докачки)
#   -BadHash   — подсунуть неверный digest (обновление должно отказаться ставиться)
# Программу запускать с переменной окружения VGD_UPDATE_URL=http://localhost:<порт>/releases.
param(
    [Parameter(Mandatory = $true)][string]$Assets,
    [Parameter(Mandatory = $true)][string]$Version,
    [int]$Port = 8765,
    [switch]$NoDigest,
    [switch]$CutOnce,
    [switch]$BadHash,
    [switch]$Prerelease
)
$ErrorActionPreference = 'Stop'
$base = "http://localhost:$Port"
$files = Get-ChildItem -LiteralPath $Assets -File
$list = foreach ($f in $files) {
    $a = [ordered]@{ name = $f.Name; size = $f.Length; browser_download_url = "$base/$([uri]::EscapeDataString($f.Name))"; digest = $null }
    if (-not $NoDigest -and $f.Extension -ne '.sha256') {
        $h = (Get-FileHash -Algorithm SHA256 -LiteralPath $f.FullName).Hash.ToLower()
        if ($BadHash) { $h = '0' * 64 }
        $a.digest = "sha256:$h"
    }
    $a
}
$body = @"
## Что нового в $Version (mock)

- проверка обновления через **mock-сервер**
- вторая строка
"@
$release = [ordered]@{
    tag_name = "v$Version"; name = "Very Good Downloader $Version"; draft = $false; prerelease = [bool]$Prerelease
    html_url = "$base/page"; published_at = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ'); body = $body
    assets = @($list)
}
$json = ConvertTo-Json -Depth 6 -InputObject @($release)
$jsonBytes = [Text.Encoding]::UTF8.GetBytes($json)

$http = New-Object Net.HttpListener
$http.Prefixes.Add("$base/")
$http.Start()
Write-Host "mock: $base/releases — файлов: $($files.Count). Задайте VGD_UPDATE_URL=$base/releases. Ctrl+C — стоп."
$cut = @{}
try {
    while ($http.IsListening) {
        $ctx = $http.GetContext()
        $req = $ctx.Request; $res = $ctx.Response
        $path = [uri]::UnescapeDataString($req.Url.AbsolutePath.TrimStart('/'))
        $range = $req.Headers['Range']
        Write-Host ("{0:HH:mm:ss} {1} /{2} {3} UA={4}" -f (Get-Date), $req.HttpMethod, $path, $range, $req.UserAgent)
        try {
            if ($path -eq 'releases') {
                $res.ContentType = 'application/json; charset=utf-8'
                $res.ContentLength64 = $jsonBytes.Length
                $res.OutputStream.Write($jsonBytes, 0, $jsonBytes.Length)
                $res.Close(); continue
            }
            $f = $files | Where-Object { $_.Name -eq $path } | Select-Object -First 1
            if (-not $f) { $res.StatusCode = 404; $res.Close(); continue }
            $from = 0L
            if ($range -match '^bytes=(\d+)-$') { $from = [long]$Matches[1] }
            if ($from -ge $f.Length -and $from -gt 0) {
                $res.StatusCode = 416; $res.AddHeader('Content-Range', "bytes */$($f.Length)"); $res.Close(); continue
            }
            if ($from -gt 0) {
                $res.StatusCode = 206
                $res.AddHeader('Content-Range', "bytes $from-$($f.Length - 1)/$($f.Length)")
            }
            $res.ContentType = 'application/octet-stream'
            $res.ContentLength64 = $f.Length - $from
            $fs = [IO.File]::OpenRead($f.FullName)
            try {
                [void]$fs.Seek($from, 'Begin')
                $stop = $f.Length
                if ($CutOnce -and -not $cut[$f.Name] -and $f.Length -gt 1MB) { $cut[$f.Name] = $true; $stop = [long]($f.Length / 2) }
                $buf = New-Object byte[] 262144
                $pos = $from
                while ($pos -lt $stop) {
                    $n = $fs.Read($buf, 0, [int][Math]::Min($buf.Length, $stop - $pos))
                    if ($n -le 0) { break }
                    $res.OutputStream.Write($buf, 0, $n); $pos += $n
                }
                if ($pos -lt $f.Length) { Write-Host "  обрыв на $pos из $($f.Length)"; $res.Abort(); continue }
            } finally { $fs.Close() }
            $res.Close()
        } catch { Write-Host "  ошибка: $_"; try { $res.Abort() } catch {} }
    }
} finally { $http.Stop() }
