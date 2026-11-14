//! Very Good Downloader для Windows: окно (vgd-gui) и консоль (vgd).
//! Ядро — `vgd_core`; его модули доступны отсюда под прежними именами (`vgd::downloader`, …).

pub use vgd_core::{downloader, fingerprint, http, media, music, net, queue, settings, sort, update, vk, yandex_acc};

pub mod appinfo;
pub mod ipc;
pub mod spotify_app;
pub mod updater;
pub mod vk_web;
pub mod watch;

/// Темы: всё из ядра + инструкция «КАК СДЕЛАТЬ ТЕМУ.html» (картинки нужны только окну).
pub mod theme {
    pub use vgd_core::theme::*;
    pub mod guide;
}
