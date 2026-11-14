# Very Good Downloader для Android

Интерфейс — Kotlin + Jetpack Compose (Material 3), ядро — тот же Rust, что у версии для Windows
(`../rust/core`), собранный в `libvgd.so` (`../rust/android`) и вызываемый через UniFFI.
yt-dlp (+ Python), ffmpeg и QuickJS — официальные сборки [youtubedl-android](https://github.com/yausername/youtubedl-android)
0.18.1 с Maven Central (`io.github.junkfood02.youtubedl-android`). План и решения — `PLAN.md`.

## Что нужно для сборки

| Что | Версия | Откуда |
|---|---|---|
| JDK | 17–21 (проверено 21) | Oracle / Adoptium |
| Android SDK | platform 36, build-tools 36.0.0, platform-tools | `cmdline-tools` от Google |
| Android NDK | 28.2.13676358 | SDK (`android sdk install ndk/28.2.13676358`) |
| Rust | stable + `aarch64-linux-android` (и `x86_64-linux-android` для эмулятора) | rustup |
| cargo-ndk | 4.x | `cargo install cargo-ndk` |

На ПК владельца всё стоит в `D:\Android` (SDK — `D:\Android\Sdk`, кеш Gradle — `D:\Android\.gradle`):
на диске C: мало места. `local.properties` (в git не кладётся): `sdk.dir=D\:/Android/Sdk`.

```powershell
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk
```

## Сборка

```powershell
cd android
$env:JAVA_HOME = "C:\Program Files\Java\jdk-21.0.11"
$env:GRADLE_USER_HOME = "D:\Android\.gradle"
.\gradlew.bat assembleDebug
```

APK: `app\build\outputs\apk\debug\app-debug.apk` (~75 МБ, только arm64-v8a).
Gradle сам собирает Rust (`cargo ndk … --profile android -p vgd-android` → `app/build/rustJniLibs`)
и Kotlin-обёртку (`uniffi-bindgen` → `app/build/generated/uniffi`).

- Для эмулятора на ПК: `.\gradlew.bat assembleDebug -Pvgd.abis=arm64-v8a,x86_64`.
- Проверки: `.\gradlew.bat lintDebug testDebugUnitTest`. `testDebugUnitTest` — связка Kotlin ↔ Rust на JVM
  (libvgd собирается под Windows, JNA). **В PATH владельца есть лишняя кавычка** (`C:\Program Files\nodejs"`),
  из-за неё тесты Gradle не запускаются (`Could not find or load main class Files\GitHub`) — запускать с
  исправленным PATH: `$env:PATH = $env:PATH -replace '"', ''` (а лучше убрать кавычку в «Переменных среды»).
- Ядро без Android: в `rust/` — `cargo test --lib` (ПК), `cargo test -p vgd-android`,
  `cargo ndk -t arm64-v8a check -p vgd-core -p vgd-android`.

## Эмулятор (Pixel 6, Android 16)

```powershell
$env:ANDROID_AVD_HOME = "D:\Android\avd"
D:\Android\Sdk\emulator\emulator.exe -avd vgd_pixel6 -no-snapshot-save -dns-server 8.8.8.8,1.1.1.1
.\gradlew.bat assembleDebug -Pvgd.abis=arm64-v8a,x86_64
D:\Android\Sdk\platform-tools\adb.exe install -r -g app\build\outputs\apk\debug\app-debug.apk
```

Без `-dns-server` у эмулятора на Windows не работает DNS (yt-dlp: «No address associated with hostname»).
Скриншоты экранов во всех стилях без эмулятора: `.\gradlew.bat testDebugUnitTest --tests *ScreensTest* -Pvgd.screens=<папка>`.

## Установка на телефон

1. На телефоне: «Настройки → О телефоне → Номер сборки» 7 раз → «Для разработчиков» → «Отладка по USB».
2. `D:\Android\Sdk\platform-tools\adb.exe install -r app-debug.apk` (или скопировать APK на телефон и открыть —
   Play Protect спросит «Всё равно установить»).
3. Первый запуск: разрешить уведомления и «Доступ ко всем файлам» (кнопка на главном экране).

Debug-сборка ставится как отдельное приложение `io.github.vivegass.vgd.debug` и подписана debug-ключом
этого ПК; свой ключ для релизов — этап 7.

## Как устроено

| Файл | Что делает |
|---|---|
| `Vgd.kt` | распаковка Python/yt-dlp/ffmpeg (youtubedl-android), пути → `init` в Rust, очередь `Core` → `StateFlow`, MediaScanner |
| `ShareActivity.kt` | «Поделиться», «Открыть с помощью» (youtube.com, youtu.be), выделенный текст: ссылка сразу в очередь, экран не открывается |
| `ClipTileService.kt` + `ClipActivity.kt` | плитка в шторке «Скачать из буфера» |
| `DownloadService.kt` | служба переднего плана `dataSync`: прогресс, WakeLock + WifiLock, `onTimeout` (Android 15) → пауза |
| `Notifications.kt` | уведомления прогресса и «скачано / не скачалось» |
| `Shortcuts.kt`, `res/xml/shortcuts.xml` | ярлыки «Видео» / «Звук»: Direct Share в «Поделиться» и на значке приложения |
| `VkAgent.kt`, `LoginActivity.kt` | VK: скрытый WebView с `vk_agent.js` (поиск), окно входа VK / YouTube (→ `cookies.txt`) |
| `SubsWorker.kt` | подписки раз в 6 ч по Wi-Fi (WorkManager, служба dataSync) |
| `ui/App.kt` | каркас, «Скачать» (очередь: превью, перетаскивание, смахнуть — убрать), «Журнал» |
| `ui/Kit.kt`, `ui/Images.kt` | элементы по стилю (заголовок, навигация, группы, переключатели, кнопки, выбор, прогресс); превью и значки платформ |
| `ui/Subs.kt`, `ui/Settings.kt` | «Подписки»; «Настройки» (загрузки, монтаж, музыка, аккаунты, папка, оформление, телефон, yt-dlp, проверка) |
| `ui/Theme.kt` | стиль по марке (Material You / One UI / HyperOS / HarmonyOS / iOS — `VgdStyle`), темы с ПК → `ColorScheme` |
| `../rust/android/src/lib.rs` | что видит Kotlin: `init`, настройки JSON, `Core` (очередь), темы, `diagnose`, VK, yt-dlp |

yt-dlp запускается как у youtubedl-android: `libpython.so <noBackupFilesDir>/youtubedl-android/yt-dlp/yt-dlp
--ffmpeg-location libffmpeg.so --js-runtimes quickjs:libqjs.so …` с `LD_LIBRARY_PATH`, `PYTHONHOME`,
`SSL_CERT_FILE` из распакованных пакетов. Программы можно запускать только из `nativeLibraryDir`, поэтому
`android:extractNativeLibs="true"` и `useLegacyPackaging`.
