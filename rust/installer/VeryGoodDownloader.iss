; Установщик Very Good Downloader — Rust-версия (Inno Setup 6). Собирается из rust\build.ps1.
; AppId тот же, что у Python-версии: установка поверх обновляет её, а не ставит вторую копию.
#define AppName "Very Good Downloader"
#ifndef AppVersion
  #define AppVersion "1.11.0"
#endif
#define AppExe "Very Good Downloader.exe"

[Setup]
AppId={{6A4E2C1B-8F3D-4B7A-9C25-7E1D0F4B3A91}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppName}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; Без прав администратора: ставится в профиль пользователя.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile=..\app.ico
UninstallDisplayIcon={app}\{#AppExe}
WizardStyle=modern
Compression=lzma2/ultra64
SolidCompression=yes
OutputDir=..\..\release
OutputBaseFilename=VeryGoodDownloader-Setup-{#AppVersion}
CloseApplications=yes

[Languages]
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "browser"; Description: "Запускать программу из расширения браузера (протокол vgd://)"; GroupDescription: "Браузер:"

[InstallDelete]
; Остатки Python-версии 1.0.x (PyInstaller): больше не нужны.
Type: filesandordirs; Name: "{app}\_internal"
Type: files; Name: "{app}\yt_dlp_version.txt"
Type: files; Name: "{app}\bin\ffprobe.exe"

[Files]
Source: "..\dist\VeryGoodDownloader\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Registry]
; Протокол vgd:// — расширение браузера запускает программу, если окно закрыто.
Root: HKCU; Subkey: "Software\Classes\vgd"; ValueType: string; ValueName: ""; ValueData: "URL:Very Good Downloader"; Flags: uninsdeletekey; Tasks: browser
Root: HKCU; Subkey: "Software\Classes\vgd"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Tasks: browser
Root: HKCU; Subkey: "Software\Classes\vgd\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"",0"; Tasks: browser
Root: HKCU; Subkey: "Software\Classes\vgd\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""; Tasks: browser

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\Расширение для браузера (папка)"; Filename: "{app}\extension"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
; Сразу после установки — предложить поставить расширение (программа откроется с выбором браузера).
Filename: "{app}\{#AppExe}"; Parameters: "--page=extension"; Description: "Запустить и установить расширение для браузера (кнопки ↓ на сайтах)"; Flags: nowait postinstall skipifsilent
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent unchecked

[UninstallDelete]
Type: filesandordirs; Name: "{app}"
