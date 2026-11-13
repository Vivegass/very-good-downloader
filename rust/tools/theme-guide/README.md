# Инструкция по темам: как пересобрать

Текст инструкции — `guide_template.html` (вставки `{{…}}` заполняет `guide_b.py`).

1. `cargo build --release` (окно программы закрыть).
2. `python guide_a.py` — готовит во временной папке (`%TEMP%gd-theme-guide`) по папке настроек на каждый скриншот.
3. PowerShell: `.\guide_shots.ps1 -Exe ..\..	argeteleasegd-gui.exe -Root $env:TEMPgd-theme-guideppdata -Out $env:TEMPgd-theme-guideaw`
   — по очереди открывает окно с каждой темой и снимает его (окна мелькают на экране ~3 минуты).
4. `python guide_b.py` — уменьшает картинки, ставит номера на «радужные», собирает
   `assets/theme-guide/КАК СДЕЛАТЬ ТЕМУ.html` + `инструкция/*.png` и `src/theme/guide_files.rs`.
5. Увеличить `VERSION` в `src/theme/guide.rs` — у пользователей инструкция обновится при следующем запуске.

Номера на картинке цветов (`MARKS` в `guide_b.py`) стоят по координатам скриншота 900×780 — после изменений в окне их надо проверить.
