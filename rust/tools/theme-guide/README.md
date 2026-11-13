# Инструкция по темам: как пересобрать

Текст инструкции — `guide_template.html` (вставки `{{…}}` заполняет `guide_b.py`).

1. `cargo build --release` (окно программы закрыть).
2. `python guide_a.py` — готовит во временной папке (`%TEMP%\vgd-theme-guide`) по папке настроек на каждый скриншот.
3. PowerShell:
   `.\guide_shots.ps1 -Exe ..\..\target\release\vgd-gui.exe -Root $env:TEMP\vgd-theme-guide\appdata -Out $env:TEMP\vgd-theme-guide\raw`
   — по очереди открывает окно с каждой темой и снимает его (окна мелькают на экране ~3 минуты).
   Файл `.ps1` должен быть в UTF-8 с BOM (иначе PowerShell 5.1 не поймёт русские строки).
4. `python guide_b.py` — уменьшает картинки, ставит номера на «радужные», собирает
   `assets/theme-guide/КАК СДЕЛАТЬ ТЕМУ.html` + `инструкция/*.png` и `desktop/src/theme/guide_files.rs`.
5. Увеличить `VERSION` в `desktop/src/theme/guide.rs` — у пользователей инструкция обновится при следующем запуске.

Номера на картинке цветов (`MARKS` в `guide_b.py`) стоят по координатам скриншота 900×780 — после изменений в окне их надо проверить.
