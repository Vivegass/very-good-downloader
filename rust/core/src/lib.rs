//! Very Good Downloader: ядро, общее для версии для Windows (`desktop`) и Android (`android`).
//! Окна, трея, реестра (кроме системного прокси) и WebView здесь нет.

pub mod appinfo;
pub mod downloader;
pub mod fingerprint;
pub mod http;
pub mod media;
pub mod music;
pub mod net;
pub mod settings;
pub mod sort;
pub mod theme;
pub mod vk;
pub mod yandex_acc;
