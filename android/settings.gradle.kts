// Very Good Downloader для Android: интерфейс на Kotlin + Jetpack Compose, ядро — Rust (../rust, libvgd.so).
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "VeryGoodDownloader"
include(":app")
