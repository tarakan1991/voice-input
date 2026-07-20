// Копирует разделяемые библиотеки llama.cpp в src-tauri/frameworks/ перед
// бандлингом (beforeBundleCommand). Оттуда их забирает бандлер:
// macOS — bundle.macOS.frameworks, Windows — bundle.resources.
//
// Отдельный node-скрипт вместо shell-однострочника: beforeBundleCommand
// выполняется и на macOS, и на Windows, общего shell-синтаксиса нет.

import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  statSync,
} from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
// CI выносит target в короткий каталог (CARGO_TARGET_DIR=C:\t): cl.exe не
// умеет длинные пути, а вложенность сборщика Vulkan-шейдеров огромна.
const targetDir =
  process.env.CARGO_TARGET_DIR ?? join(root, "src-tauri", "target");
const release = join(targetDir, "release");
const frameworks = join(root, "src-tauri", "frameworks");
mkdirSync(frameworks, { recursive: true });

let copied = 0;
function copyMatching(dir, pattern) {
  for (const f of readdirSync(dir).filter((f) => pattern.test(f))) {
    copyFileSync(join(dir, f), join(frameworks, f));
    console.log(`sync-libs: ${f}`);
    copied++;
  }
}

if (process.platform === "win32") {
  // На Windows DLL кладёт в инсталлер NSIS-хук (installer-hooks.nsh) из
  // target/release. Но llama-cpp-sys-2 хардлинкает DLL туда только «если
  // файла ещё нет» — после смены флагов сборки там остаются устаревшие
  // копии. Поэтому перед бандлингом освежаем их из самого свежего OUT_DIR.
  const buildDir = join(release, "build");
  const outBins = readdirSync(buildDir)
    .filter((d) => d.startsWith("llama-cpp-sys-2-"))
    .map((d) => join(buildDir, d, "out", "bin"))
    .filter((p) => existsSync(p))
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  if (outBins.length === 0) {
    console.error("sync-libs: сборка llama-cpp-sys-2 не найдена");
    process.exit(1);
  }
  for (const f of readdirSync(outBins[0]).filter((f) => f.endsWith(".dll"))) {
    copyFileSync(join(outBins[0], f), join(release, f));
    console.log(`sync-libs: ${f} → target/release`);
    copied++;
  }
} else {
  // macOS: llama-cpp-sys-2 хардлинкает dylib в target/release — берём там же,
  // как делал исходный shell-однострожник.
  copyMatching(release, /^lib(llama|llama-common|ggml)[^/]*\.0\.dylib$/);
}

if (copied === 0) {
  console.error("sync-libs: библиотеки llama.cpp не найдены");
  process.exit(1);
}
