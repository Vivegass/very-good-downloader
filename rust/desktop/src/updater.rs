//! Установка скачанного обновления на Windows (поиск и скачивание — `vgd_core::update`).
//!
//! - Установленная программа (рядом есть `unins000.exe`) — установщик Inno Setup в тихом режиме
//!   `/SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /VGDRELAUNCH=window|background`;
//!   после установки он сам запускает программу снова (installer/VeryGoodDownloader.iss, [Run]).
//! - Без установки (portable) — zip распаковывается во временную папку заранее, а после выхода программы
//!   маленький скрипт PowerShell копирует файлы поверх и запускает её снова.
//!
//! И установщик, и копирование запускает вспомогательный скрипт, который сначала ждёт, пока эта копия
//! программы выйдет: мьютекс одной копии и порт 38517 освобождаются, и новая версия стартует как обычно.
//! Что происходило — в `<данные>\update\update.log`.

use std::path::{Path, PathBuf};
use std::process::Command;

use vgd_core::appinfo::{app_dir, APP_VERSION};
use vgd_core::update::{self, Platform};

/// Имя exe окна в установленной/распакованной программе.
pub const GUI_EXE: &str = "Very Good Downloader.exe";

/// Какая это сборка: установленная (есть деинсталлятор Inno Setup) или portable.
pub fn platform() -> Platform {
    if app_dir().join("unins000.exe").is_file() {
        Platform::WindowsSetup
    } else {
        Platform::WindowsPortable
    }
}

/// Скачанное и подготовленное обновление.
#[derive(Clone, Debug, PartialEq)]
pub enum Ready {
    /// установщик `VeryGoodDownloader-Setup-<ver>.exe`
    Installer(PathBuf),
    /// распакованная папка с новой версией (в ней лежит `Very Good Downloader.exe`)
    Portable(PathBuf),
}

/// Файл, по которому новая версия после запуска узнаёт, что её только что поставили.
fn marker() -> PathBuf {
    update::dir().join("pending.json")
}

/// Журнал вспомогательного скрипта и установщика.
pub fn log_file() -> PathBuf {
    update::dir().join("update.log")
}

/// Подготовить скачанный файл: установщик — как есть, zip — распаковать рядом.
pub fn prepare(platform: Platform, file: &Path, version: &str) -> Result<Ready, String> {
    match platform {
        Platform::WindowsPortable => {
            let dest = update::dir().join(format!("portable-{version}"));
            let _ = std::fs::remove_dir_all(&dest);
            unzip(file, &dest)?;
            find_app(&dest).map(Ready::Portable).ok_or_else(|| format!("в архиве нет «{GUI_EXE}»"))
        }
        _ => Ok(Ready::Installer(file.to_path_buf())),
    }
}

/// Распаковать zip средствами .NET (есть в любой Windows 10/11; имена в UTF-8 читает правильно,
/// пути за пределами папки не пропускает).
fn unzip(zip: &Path, dest: &Path) -> Result<(), String> {
    let script = "$ErrorActionPreference='Stop'; Add-Type -AssemblyName System.IO.Compression.FileSystem; \
        [IO.Compression.ZipFile]::ExtractToDirectory($env:VGD_ZIP, $env:VGD_DEST)";
    let out = vgd_core::appinfo::command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
        .env("VGD_ZIP", zip)
        .env("VGD_DEST", dest)
        .output()
        .map_err(|e| format!("powershell: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("ошибка распаковки");
        return Err(format!("не удалось распаковать: {line}"));
    }
    Ok(())
}

/// Папка с `Very Good Downloader.exe`: сам корень архива или папка внутри (`VeryGoodDownloader\`).
fn find_app(root: &Path) -> Option<PathBuf> {
    if root.join(GUI_EXE).is_file() {
        return Some(root.to_path_buf());
    }
    std::fs::read_dir(root).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).find_map(|p| find_app_shallow(&p))
}

fn find_app_shallow(dir: &Path) -> Option<PathBuf> {
    dir.join(GUI_EXE).is_file().then(|| dir.to_path_buf())
}

/// Скрипт-помощник: ждёт выхода программы, ставит обновление, запускает программу снова.
/// Параметры — через переменные окружения (пути с пробелами и кириллицей без экранирования).
const HELPER: &str = r#"
$ErrorActionPreference = 'Stop'
$log = $env:VGD_UPD_LOG; $mode = $env:VGD_UPD_MODE; $src = $env:VGD_UPD_SRC; $dst = $env:VGD_UPD_DST
$exe = $env:VGD_UPD_EXE; $bg = $env:VGD_UPD_BG; $procId = [int]$env:VGD_UPD_PID
foreach ($n in 'LOG','MODE','SRC','DST','EXE','BG','PID') { Remove-Item "Env:VGD_UPD_$n" -ErrorAction SilentlyContinue }
function L($m) { try { Add-Content -LiteralPath $log -Encoding UTF8 -Value ((Get-Date -Format 'yyyy-MM-dd HH:mm:ss') + ' ' + $m) } catch {} }
function Run-App($path) {
    if ($bg -eq 'background') { Start-Process -FilePath $path -ArgumentList '--background' -WorkingDirectory (Split-Path $path) }
    else { Start-Process -FilePath $path -WorkingDirectory (Split-Path $path) }
}
try {
    L "обновление ($mode): жду выхода программы, pid $procId"
    try { Wait-Process -Id $procId -Timeout 90 -ErrorAction Stop } catch {}
    if (Get-Process -Id $procId -ErrorAction SilentlyContinue) {
        L 'программа не вышла за 90 с — закрываю'
        Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 2
    }
    if ($mode -eq 'setup') {
        $setupLog = Join-Path (Split-Path $log) 'setup.log'
        $a = '/SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /VGDRELAUNCH=' + $bg + ' /LOG="' + $setupLog + '"'
        L "запускаю установщик: $src $a"
        $p = Start-Process -FilePath $src -ArgumentList $a -PassThru -Wait
        L "установщик закончил, код $($p.ExitCode)"
        # Установщик сам запускает программу (VGDRELAUNCH). Не установилось — вернуть старую.
        if ($p.ExitCode -ne 0) { Run-App $exe }
    } else {
        $ok = $false
        for ($i = 0; $i -lt 30 -and -not $ok; $i++) {
            try {
                Get-ChildItem -LiteralPath $src -Force | Copy-Item -Destination $dst -Recurse -Force
                $ok = $true
            } catch { L "копирование: $($_.Exception.Message) — ещё раз"; Start-Sleep -Seconds 1 }
        }
        if ($ok) { L "файлы заменены: $dst" } else { L 'не удалось заменить файлы' }
        Remove-Item -LiteralPath $src -Recurse -Force -ErrorAction SilentlyContinue
        Run-App $exe
        L "запущено: $exe"
    }
} catch {
    L "ошибка: $($_.Exception.Message)"
    try { Run-App $exe } catch { L "не удалось запустить: $($_.Exception.Message)" }
}
"#;

/// Запустить установку и вернуться: дальше программа должна выйти сама (скрипт ждёт её выхода).
/// `background` — после обновления запустить снова в трее, без окна.
pub fn launch(ready: &Ready, version: &str, background: bool) -> Result<(), String> {
    let dir = update::dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let app = app_dir();
    let (mode, src) = match ready {
        Ready::Installer(p) => ("setup", p.clone()),
        Ready::Portable(p) => ("portable", p.clone()),
    };
    if !src.exists() {
        return Err(format!("нет {}", src.display()));
    }
    // Запустить снова — то же окно, что сейчас (в portable — каким оно будет после копирования).
    let exe = match ready {
        Ready::Portable(_) => app.join(GUI_EXE),
        Ready::Installer(_) => std::env::current_exe().unwrap_or_else(|_| app.join(GUI_EXE)),
    };
    let mark = serde_json::json!({ "from": APP_VERSION, "to": version, "mode": mode, "at": update::now() });
    std::fs::write(marker(), mark.to_string()).map_err(|e| e.to_string())?;
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", HELPER])
        .env("VGD_UPD_LOG", log_file())
        .env("VGD_UPD_MODE", mode)
        .env("VGD_UPD_SRC", &src)
        .env("VGD_UPD_DST", &app)
        .env("VGD_UPD_EXE", &exe)
        .env("VGD_UPD_BG", if background { "background" } else { "window" })
        .env("VGD_UPD_PID", std::process::id().to_string())
        .current_dir(&dir);
    spawn_detached(&mut c).map_err(|e| {
        let _ = std::fs::remove_file(marker());
        format!("не удалось запустить установку: {e}")
    })
}

/// Процесс, который переживёт выход программы (и, если можно, выйдет из её job-объекта).
#[cfg(windows)]
fn spawn_detached(c: &mut Command) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let base = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
    match c.creation_flags(base | CREATE_BREAKAWAY_FROM_JOB).spawn() {
        Ok(_) => Ok(()),
        // job-объект не разрешает выходить из него — запускаем просто так
        Err(_) => c.creation_flags(base).spawn().map(|_| ()),
    }
}

#[cfg(not(windows))]
fn spawn_detached(c: &mut Command) -> std::io::Result<()> {
    c.spawn().map(|_| ())
}

/// Чем закончилось обновление, запущенное прошлой копией программы.
#[derive(Debug, PartialEq)]
pub enum AfterUpdate {
    /// обновились: с какой версии на какую
    Updated { from: String, to: String },
    /// версия не та — не поставилось (подробности в update.log)
    Failed { to: String, log: String },
}

/// Вызывается при запуске: было ли обновление, и убрать остатки (старые установщики, распакованные папки).
/// Недокачанные `.part` и файлы версии `keep` остаются.
pub fn after_start(keep: Option<&str>) -> Option<AfterUpdate> {
    let dir = update::dir();
    let result = std::fs::read_to_string(marker()).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).map(|m| {
        let to = m["to"].as_str().unwrap_or("").to_string();
        let from = m["from"].as_str().unwrap_or("").to_string();
        if to == APP_VERSION {
            AfterUpdate::Updated { from, to }
        } else {
            AfterUpdate::Failed { to, log: log_tail(8) }
        }
    });
    let _ = std::fs::remove_file(marker());
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let keep_it = name.ends_with(".part")
                || name == "update.log"
                || name == "setup.log"
                || keep.is_some_and(|k| !k.is_empty() && name.contains(&format!("-{k}")) && !name.starts_with("portable-"));
            if keep_it {
                continue;
            }
            let p = e.path();
            let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        }
    }
    // журнал не растёт бесконечно
    if std::fs::metadata(log_file()).is_ok_and(|m| m.len() > 256 << 10) {
        let _ = std::fs::remove_file(log_file());
    }
    result
}

/// Последние строки журнала обновления.
pub fn log_tail(n: usize) -> String {
    let t = std::fs::read_to_string(log_file()).unwrap_or_default();
    let t = t.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = t.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_app_in_root_or_subfolder() {
        let d = std::env::temp_dir().join(format!("vgd-updater-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("VeryGoodDownloader")).unwrap();
        assert!(find_app(&d).is_none());
        std::fs::write(d.join("VeryGoodDownloader").join(GUI_EXE), b"x").unwrap();
        assert_eq!(find_app(&d).unwrap(), d.join("VeryGoodDownloader"));
        std::fs::write(d.join(GUI_EXE), b"x").unwrap();
        assert_eq!(find_app(&d).unwrap(), d);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(windows)]
    #[test]
    fn unzip_with_dotnet() {
        let d = std::env::temp_dir().join(format!("vgd-unzip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src").join("VeryGoodDownloader").join("bin")).unwrap();
        std::fs::write(d.join("src").join("VeryGoodDownloader").join(GUI_EXE), b"new").unwrap();
        std::fs::write(d.join("src").join("VeryGoodDownloader").join("Как установить.txt"), "текст").unwrap();
        let zip = d.join("p.zip");
        let mk = "Add-Type -AssemblyName System.IO.Compression.FileSystem; \
            [IO.Compression.ZipFile]::CreateFromDirectory($env:S, $env:Z, 'Optimal', $false)";
        let ok = vgd_core::appinfo::command("powershell")
            .args(["-NoProfile", "-Command", mk])
            .env("S", d.join("src"))
            .env("Z", &zip)
            .status()
            .unwrap();
        assert!(ok.success());
        unzip(&zip, &d.join("out")).unwrap();
        let app = find_app(&d.join("out")).unwrap();
        assert_eq!(std::fs::read(app.join(GUI_EXE)).unwrap(), b"new");
        assert!(app.join("Как установить.txt").is_file());
        assert!(unzip(&d.join("нет.zip"), &d.join("out2")).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
