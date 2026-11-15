import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// ABI из gradle.properties / -Pvgd.abis=…: arm64-v8a — телефоны, x86_64 — эмулятор на ПК.
val abis: List<String> = (project.findProperty("vgd.abis") as String? ?: "arm64-v8a")
    .split(',').map { it.trim() }.filter { it.isNotEmpty() }

// Версия приложения = версия ядра и ПК: [workspace.package] version в rust/Cargo.toml.
val workspaceVersion: String = rootProject.file("../rust/Cargo.toml").readLines()
    .first { it.trim().startsWith("version = \"") }.substringAfter('"').substringBefore('"')

android {
    namespace = "io.github.vivegass.vgd"
    compileSdk = 36
    ndkVersion = "28.2.13676358"

    defaultConfig {
        applicationId = "io.github.vivegass.vgd"
        minSdk = 26
        targetSdk = 36
        // Версия общая с ПК — из rust/Cargo.toml (один релиз v<версия> на обе платформы);
        // versionCode = 1.13.0 → 11300. -Pvgd.versionName=… -Pvgd.versionCode=… — проверка обновления (mock-релиз).
        versionName = (project.findProperty("vgd.versionName") as String?) ?: workspaceVersion
        versionCode = (project.findProperty("vgd.versionCode") as String?)?.toInt()
            ?: workspaceVersion.substringBefore('-').split('.').map { it.toInt() }.let { (a, b, c) -> a * 10000 + b * 100 + c }
        ndk { abiFilters += abis }
    }

    // Свой ключ подписи для release (этап 7): android/keystore.properties (в git не кладётся) —
    //   storeFile=D:/keys/vgd-release.jks  storePassword=…  keyAlias=vgd  keyPassword=…
    // Обновление ставится поверх только при том же ключе: ключ нужен ДО первого публичного APK и терять его нельзя.
    val ksFile = rootProject.file("keystore.properties")
    val releaseKey = if (ksFile.isFile) {
        val p = Properties().apply { ksFile.inputStream().use { load(it) } }
        signingConfigs.create("release") {
            storeFile = file(p.getProperty("storeFile"))
            storePassword = p.getProperty("storePassword")
            keyAlias = p.getProperty("keyAlias")
            keyPassword = p.getProperty("keyPassword")
        }
    } else null

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
        }
        release {
            isMinifyEnabled = false
            // Без keystore.properties — debug-ключ этого ПК (только для проверки, не для публикации).
            signingConfig = releaseKey ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    // youtubedl-android запускает libpython.so / libffmpeg.so / libqjs.so как программы:
    // они должны лежать распакованными в nativeLibraryDir.
    packaging {
        jniLibs { useLegacyPackaging = true }
    }

    // скриншот-тесты (Robolectric + Roborazzi): ресурсы Android нужны и на JVM
    testOptions { unitTests { isIncludeAndroidResources = true } }

    sourceSets["main"].jniLibs.srcDir(layout.buildDirectory.dir("rustJniLibs"))
    sourceSets["main"].java.srcDir(layout.buildDirectory.dir("generated/uniffi"))
}

kotlin {
    compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) }
}

// ---------- Rust: libvgd.so (cargo-ndk) и Kotlin-обёртка (uniffi-bindgen) ----------

val rustDir = rootProject.file("../rust")
val cargo: String = run {
    val home = System.getProperty("user.home")
    val exe = if (System.getProperty("os.name").lowercase().contains("win")) "cargo.exe" else "cargo"
    val f = File(home, ".cargo/bin/$exe")
    if (f.exists()) f.absolutePath else "cargo"
}
val rustJniLibs = layout.buildDirectory.dir("rustJniLibs")
val uniffiOut = layout.buildDirectory.dir("generated/uniffi")
val ndkDir: String = run {
    val props = Properties()
    rootProject.file("local.properties").takeIf { it.exists() }?.inputStream()?.use { props.load(it) }
    val sdk = props.getProperty("sdk.dir") ?: System.getenv("ANDROID_HOME") ?: System.getenv("ANDROID_SDK_ROOT") ?: ""
    File(sdk, "ndk/${android.ndkVersion}").absolutePath
}

val cargoNdk by tasks.registering(Exec::class) {
    description = "Собрать libvgd.so (Rust) для ${abis.joinToString()}"
    workingDir = rustDir
    environment("ANDROID_NDK_HOME", ndkDir)
    val targets = abis.flatMap { listOf("-t", it) }
    commandLine(listOf(cargo, "ndk") + targets + listOf("-P", "26", "-o", rustJniLibs.get().asFile.absolutePath,
        "build", "--profile", "android", "-p", "vgd-android"))
    inputs.dir(rustDir.resolve("core/src"))
    inputs.dir(rustDir.resolve("android/src"))
    inputs.files(rustDir.resolve("Cargo.toml"), rustDir.resolve("Cargo.lock"), rustDir.resolve("core/Cargo.toml"), rustDir.resolve("android/Cargo.toml"))
    inputs.property("abis", abis)
    outputs.dir(rustJniLibs)
}

val uniffiBindgen by tasks.registering(Exec::class) {
    description = "Kotlin-обёртка для libvgd.so (UniFFI)"
    dependsOn(cargoNdk)
    workingDir = rustDir
    val triple = when (abis.first()) {
        "x86_64" -> "x86_64-linux-android"
        else -> "aarch64-linux-android"
    }
    commandLine(cargo, "run", "-q", "-p", "uniffi-bindgen", "--", "generate",
        "--library", "target/$triple/android/libvgd.so", "--language", "kotlin",
        "--config", "android/uniffi.toml", "--out-dir", uniffiOut.get().asFile.absolutePath, "--no-format")
    inputs.files(cargoNdk)
    inputs.file(rustDir.resolve("android/uniffi.toml"))
    outputs.dir(uniffiOut)
}

tasks.named("preBuild") { dependsOn(uniffiBindgen) }

// JVM-тесты (src/test): та же Kotlin-обёртка, libvgd собрана под ПК — проверка связки без телефона.
val cargoHost by tasks.registering(Exec::class) {
    description = "libvgd для ПК (тесты обёртки на JVM)"
    workingDir = rustDir
    commandLine(cargo, "build", "--profile", "android", "-p", "vgd-android")
}
tasks.withType<Test>().configureEach {
    dependsOn(cargoHost)
    systemProperty("jna.library.path", rustDir.resolve("target/android").absolutePath)
    // скриншоты стилей: -Pvgd.screens=<папка> (по умолчанию build/screens)
    systemProperty("roborazzi.test.record", "true")
    systemProperty("vgd.screens", (project.findProperty("vgd.screens") as String?) ?: layout.buildDirectory.dir("screens").get().asFile.absolutePath)
    maxHeapSize = "3g"
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2025.12.01")
    implementation(composeBom)
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    debugImplementation("androidx.compose.ui:ui-tooling")
    implementation("androidx.activity:activity-compose:1.11.0")
    implementation("androidx.core:core-ktx:1.17.0")
    // ярлыки «Поделиться» (Direct Share) в верхнем ряду системной панели
    implementation("androidx.sharetarget:sharetarget:1.2.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.4")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    // подписки по расписанию
    implementation("androidx.work:work-runtime-ktx:2.10.5")

    // yt-dlp + Python, ffmpeg, QuickJS — официальные сборки youtubedl-android (Maven Central)
    val ytdl = "0.18.1"
    implementation("io.github.junkfood02.youtubedl-android:library:$ytdl")
    implementation("io.github.junkfood02.youtubedl-android:ffmpeg:$ytdl")

    // UniFFI-обёртка вызывает libvgd.so через JNA
    implementation("net.java.dev.jna:jna:5.18.1@aar")

    testImplementation("junit:junit:4.13.2")
    testImplementation("net.java.dev.jna:jna:5.18.1")
    testImplementation("org.json:json:20250517")
    // скриншоты экранов во всех стилях без телефона и эмулятора
    testImplementation(composeBom)
    testImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
    testImplementation("org.robolectric:robolectric:4.16.1")
    testImplementation("io.github.takahirom.roborazzi:roborazzi:1.75.0")
    testImplementation("io.github.takahirom.roborazzi:roborazzi-compose:1.75.0")
}
