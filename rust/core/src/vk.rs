//! Поиск музыки в VK от аккаунта пользователя — общий интерфейс.
//!
//! VK отдаёт музыку только странице vk.com с вошедшим пользователем, поэтому поиск делает «невидимый браузер»:
//! на Windows — отдельный процесс с WebView2 (`desktop/src/vk_web.rs`), на Android — скрытый WebView на Kotlin.
//! Оба выполняют один и тот же `vk_agent.js`. Ядро знает только этот trait; реализацию ставит программа
//! при запуске (`set_backend`). Пароль программа не видит: пользователь входит на самой странице vk.com.

use std::sync::{Arc, RwLock};

/// Код, который выполняется на странице vk.com (поиск, расшифровка ссылок на звук). Общий для WebView2 и Android.
pub const AGENT_JS: &str = include_str!("vk_agent.js");

/// Найденный в VK трек: ссылка на звук (HLS), название, исполнитель, длительность, обложка.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VkHit {
    pub url: String,
    pub title: String,
    pub artist: String,
    pub duration: Option<f64>,
    pub cover: Option<String>,
}

/// «Невидимый браузер» с vk.com.
pub trait VkBackend: Send + Sync {
    /// Поиск треков (`audio.search` от имени вошедшего пользователя).
    fn search(&self, q: &str, count: u32) -> Result<Vec<VkHit>, String>;
    /// Вошёл ли пользователь (по последней проверке).
    fn logged_in(&self) -> bool;
}

static BACKEND: RwLock<Option<Arc<dyn VkBackend>>> = RwLock::new(None);

/// Поставить реализацию (один раз при запуске программы; повторный вызов заменяет прежнюю).
pub fn set_backend(b: Arc<dyn VkBackend>) {
    *BACKEND.write().unwrap_or_else(|e| e.into_inner()) = Some(b);
}

fn backend() -> Option<Arc<dyn VkBackend>> {
    BACKEND.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Поиск треков в VK. Нет реализации — понятная ошибка, музыка ищется в других местах.
pub fn search(q: &str, count: u32) -> Result<Vec<VkHit>, String> {
    match backend() {
        Some(b) => b.search(q, count),
        None => Err("поиск в VK недоступен в этой версии программы".into()),
    }
}

pub fn logged_in() -> bool {
    backend().is_some_and(|b| b.logged_in())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl VkBackend for Fake {
        fn search(&self, q: &str, count: u32) -> Result<Vec<VkHit>, String> {
            Ok((0..count).map(|i| VkHit { title: format!("{q} {i}"), ..Default::default() }).collect())
        }
        fn logged_in(&self) -> bool {
            true
        }
    }

    #[test]
    fn dispatches_to_backend() {
        set_backend(Arc::new(Fake));
        let r = search("песня", 2).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].title, "песня 1");
        assert!(logged_in());
    }
}
