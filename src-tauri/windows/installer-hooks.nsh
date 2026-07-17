; Хуки NSIS-инсталлера (bundle.windows.nsis.installerHooks).
;
; 1) DLL рядом с exe. llama.cpp собирается динамически (риск R-13 SPEC.md),
;    и exe импортирует llama*/ggml* DLL; onnxruntime статический, но требует
;    DirectML.dll. Файлы берутся из target/release — там их создаёт сборка
;    (llama-cpp-sys-2 хардлинкает свои DLL, ort-sys кладёт DirectML.dll).
;    Пути относительные: makensis запускается в target/release/nsis/x64.
;    Через bundle.resources так нельзя: tauri_build валидирует и копирует
;    ресурсы во время cargo-сборки, когда DLL ещё не существуют (гонка
;    build-скриптов — подробности в build.rs).
;
; 2) Опция автозапуска при установке (SPEC.md §12). Оба слота чекбоксов
;    finish-страницы шаблона Tauri заняты («ярлык» и «запустить приложение»),
;    поэтому опция реализована вопросом в конце установки. Имя значения в
;    реестре обязано совпадать с tauri-plugin-autostart
;    (app.package_info().name = productName «VoiceInput») — тогда тумблер
;    «Автозапуск» в настройках видит и изменяет то же значение.

!macro NSIS_HOOK_POSTINSTALL
  SetOutPath "$INSTDIR"
  File "..\..\llama.dll"
  File "..\..\llama-common.dll"
  File "..\..\ggml.dll"
  File "..\..\ggml-base.dll"
  File "..\..\ggml-cpu.dll"
  File "..\..\DirectML.dll"

  ${IfNot} ${Silent}
    ; IDNO +2 — пропустить следующую инструкцию (WriteRegStr).
    MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON1 \
      "Запускать VoiceInput автоматически при входе в Windows?$\r$\n$\r$\nЭто можно изменить позже в настройках приложения." \
      IDNO +2
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "VoiceInput" '"$INSTDIR\${MAINBINARYNAME}.exe"'
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\llama.dll"
  Delete "$INSTDIR\llama-common.dll"
  Delete "$INSTDIR\ggml.dll"
  Delete "$INSTDIR\ggml-base.dll"
  Delete "$INSTDIR\ggml-cpu.dll"
  Delete "$INSTDIR\DirectML.dll"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "VoiceInput"
!macroend
