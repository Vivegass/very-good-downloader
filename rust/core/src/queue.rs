//! Очередь загрузок — общая для окна на ПК (`vgd-gui`) и Android (фоновая служба).
//!
//! Карточки (`Job`) в порядке очереди; рабочие нити берут из неё ссылки и качают через `Downloader`:
//! - основная нить — всё по порядку;
//! - вторая нить (`side`) — только небольшая музыка (`music::is_small_music`), пока основная занята;
//! - пауза = отмена текущего процесса с пометкой «ради паузы»: «продолжить» ставит ссылку обратно,
//!   скачанное пропускается по архиву, недокачанный `.part` yt-dlp продолжает;
//! - порядок — `move_job`; очередь нитей пересобирается из карточек в `sync`.
//!
//! Нити не трогают карточки: они шлют `Event` в `sink`, а хозяин очереди (окно или служба) передаёт
//! событие в `handle` в своей нити. Так окну на ПК не нужен замок на каждый кадр; на Android то же
//! делает `Runner` (своя нить-насос + слушатель).
//!
//! Фоновый режим ПК (`--background`, ссылки из браузера без окна): заявки `POST /add` смотрят на
//! состояние своих карточек через `ticket_state`.

use std::collections::{HashSet, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::downloader::{CancelHandle, DlError, Downloader, Progress};
use crate::settings::{JobOpts, Settings};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Queued,
    Running,
    Paused,
    Done,
    Error,
    Cancelled,
}

impl State {
    /// Ещё не закончена: ждёт, качается или на паузе.
    pub fn is_active(self) -> bool {
        matches!(self, State::Queued | State::Running | State::Paused)
    }
}

/// Карточка загрузки.
#[derive(Clone, Debug)]
pub struct Job {
    pub id: u64,
    pub url: String,
    pub title: String,
    /// "queued", "info", "video", …, "done", "error", "cancel", "paused"
    pub stage: String,
    pub item: String,
    pub percent: f32,
    pub speed: String,
    pub eta: String,
    pub message: String,
    pub files: Vec<String>,
    pub state: State,
    pub error: String,
    /// важная подсказка от загрузчика (например, про 100 треков Spotify)
    pub notice: String,
    /// фрагмент / только звук / параметры подписки / пресет
    pub opts: JobOpts,
    /// загрузка из подписки (её ссылка)
    pub sub_url: Option<String>,
}

/// Что сообщают рабочие нити.
#[derive(Clone, Debug)]
pub enum Event {
    Progress(u64, Progress),
    Log(String),
    /// id, чем закончилось, файлы, ошибка
    Finish(u64, State, Vec<String>, Option<String>),
    /// нить закончила работу (очередь пуста или всё остановлено)
    Idle,
}

/// Куда нити шлют события. Окно ещё и перерисовывается по каждому.
pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

/// Что значит событие для хозяина очереди (после `handle`).
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// карточка обновилась
    Progress(u64),
    /// строка для журнала
    Log(String),
    /// загрузка закончилась — состояние в `job(id)`
    Finished(u64),
    /// нить остановилась; если в очереди что-то осталось — уже запущена снова
    Idle,
}

/// Состояние заявки из браузера / Spotify (`GET /status`) или уведомления на телефоне.
#[derive(Clone, Debug, PartialEq)]
pub enum TicketState {
    /// что-то качается: процент текущей
    Running(f32),
    /// ждёт или на паузе
    Queued,
    /// закончилась: что-то скачано или всё уже было
    Done(String),
    Error(String),
}

/// Общее между очередью и рабочими нитями.
#[derive(Default)]
struct Shared {
    /// ждут загрузки, в порядке карточек (пересобирается в `sync`)
    queue: VecDeque<(u64, String, JobOpts)>,
    /// что качается сейчас: основная нить и нить для небольшой музыки
    current: Vec<(u64, CancelHandle)>,
    /// взяты нитями, но карточка ещё «в очереди» (первое событие не дошло до `handle`):
    /// `sync` не должен ставить их в очередь второй раз — иначе одна ссылка качается дважды
    taken: HashSet<u64>,
    settings: Settings,
    stop_all: bool,
}

pub struct Queue {
    pub jobs: Vec<Job>,
    next_id: u64,
    shared: Arc<Mutex<Shared>>,
    worker: Option<JoinHandle<()>>,
    /// вторая нить: небольшая музыка, пока основная занята длинной загрузкой
    side_worker: Option<JoinHandle<()>>,
    /// загрузки, которые останавливаем ради паузы (а не отмены)
    pausing: HashSet<u64>,
    sink: Sink,
}

impl Queue {
    pub fn new(settings: Settings, sink: Sink) -> Self {
        Queue {
            jobs: Vec::new(),
            next_id: 1,
            shared: Arc::new(Mutex::new(Shared { settings, ..Default::default() })),
            worker: None,
            side_worker: None,
            pausing: HashSet::new(),
            sink,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Настройки для следующих загрузок (текущая докачается со старыми).
    pub fn set_settings(&self, s: Settings) {
        self.lock().settings = s;
    }

    pub fn job(&self, id: u64) -> Option<&Job> {
        self.jobs.iter().find(|j| j.id == id)
    }

    /// Поставить ссылку в очередь (не запускает — см. `start`). `title` — своё название карточки
    /// (подписка); иначе название списка треков из браузера или сама ссылка.
    pub fn add(&mut self, url: String, opts: JobOpts, sub_url: Option<String>, title: Option<String>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.jobs.push(Job {
            id,
            // Список треков из браузера: вместо «vgdlist:C:\…» — название плейлиста.
            title: title.or_else(|| crate::music::list_title(&url)).unwrap_or_else(|| url.clone()),
            url,
            stage: "queued".into(),
            item: String::new(),
            percent: 0.0,
            speed: String::new(),
            eta: String::new(),
            message: String::new(),
            files: Vec::new(),
            state: State::Queued,
            error: String::new(),
            notice: String::new(),
            opts,
            sub_url,
        });
        self.sync();
        id
    }

    /// Очередь для рабочих нитей — ссылки «в очереди» в порядке карточек.
    pub fn sync(&mut self) {
        let mut sh = self.lock();
        let q: VecDeque<_> = self
            .jobs
            .iter()
            .filter(|j| j.state == State::Queued && !sh.taken.contains(&j.id))
            .map(|j| (j.id, j.url.clone(), j.opts.clone()))
            .collect();
        sh.queue = q;
    }

    /// Идёт загрузка (в любой из двух нитей).
    pub fn busy(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| !w.is_finished()) || self.side_worker.as_ref().is_some_and(|w| !w.is_finished())
    }

    /// Есть ли что-то в очереди или в работе.
    pub fn active(&self) -> bool {
        self.jobs.iter().any(|j| matches!(j.state, State::Queued | State::Running))
    }

    /// Запустить нити, если есть что качать.
    pub fn start(&mut self) {
        if !self.worker.as_ref().is_some_and(|w| !w.is_finished()) {
            self.lock().stop_all = false;
            let (shared, sink) = (self.shared.clone(), self.sink.clone());
            self.worker = Some(std::thread::spawn(move || work(shared, sink, false)));
        }
        // Небольшую музыку качаем рядом с основной нитью, не дожидаясь очереди.
        let side_busy = self.side_worker.as_ref().is_some_and(|w| !w.is_finished());
        let small = self.lock().queue.iter().any(|(_, u, _)| crate::music::is_small_music(u));
        if !side_busy && small {
            let (shared, sink) = (self.shared.clone(), self.sink.clone());
            self.side_worker = Some(std::thread::spawn(move || work(shared, sink, true)));
        }
    }

    fn cancel_running(&self, id: u64) -> bool {
        let c = self.lock().current.iter().find(|(cid, _)| *cid == id).map(|(_, c)| c.clone());
        match c {
            Some(c) => {
                std::thread::spawn(move || c.cancel());
                true
            }
            None => false,
        }
    }

    /// Отменить. true — отменяется идущая загрузка (закончится событием `Finish`).
    pub fn cancel(&mut self, id: u64) -> bool {
        let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) else { return false };
        match j.state {
            State::Queued | State::Paused => {
                j.state = State::Cancelled;
                j.stage = "cancel".into();
                j.message.clear();
                self.sync();
                false
            }
            State::Running => {
                self.pausing.remove(&id);
                self.cancel_running(id)
            }
            _ => false,
        }
    }

    /// Пауза: ссылка остаётся на своём месте, «продолжить» докачает с того же места
    /// (скачанное пропустится по архиву, недокачанный файл yt-dlp продолжит).
    pub fn pause(&mut self, id: u64) {
        let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) else { return };
        match j.state {
            State::Queued => {
                j.state = State::Paused;
                j.stage = "paused".into();
                self.sync();
            }
            State::Running => {
                j.message = "ставлю на паузу…".into();
                self.pausing.insert(id);
                self.cancel_running(id);
            }
            _ => {}
        }
    }

    /// Поставить на паузу всё (на Android — по `onTimeout` службы): ждущие и идущие.
    pub fn pause_all(&mut self) {
        let ids: Vec<u64> = self.jobs.iter().filter(|j| matches!(j.state, State::Queued | State::Running)).map(|j| j.id).collect();
        for id in ids {
            self.pause(id);
        }
    }

    pub fn resume(&mut self, id: u64) {
        if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id && j.state == State::Paused) {
            j.state = State::Queued;
            j.stage = "queued".into();
            j.message.clear();
        }
        self.sync();
        self.start();
    }

    /// Сдвинуть ждущую карточку вверх (-1) или вниз (+1) — мимо ближайшей другой ждущей
    /// («в очереди» или «на паузе»): иначе обмен с готовой или идущей карточкой ничего не меняет
    /// в порядке загрузки.
    pub fn move_job(&mut self, id: u64, dir: i32) {
        let waiting = |j: &Job| matches!(j.state, State::Queued | State::Paused);
        let Some(i) = self.jobs.iter().position(|j| j.id == id) else { return };
        if !waiting(&self.jobs[i]) {
            return;
        }
        let k = if dir < 0 {
            self.jobs[..i].iter().rposition(waiting)
        } else {
            self.jobs[i + 1..].iter().position(waiting).map(|k| k + i + 1)
        };
        if let Some(k) = k {
            self.jobs.swap(i, k);
            self.sync();
        }
    }

    /// Новый порядок карточек (перетаскивание): `ids` — как на экране; неизвестные id пропускаются,
    /// карточки, которых нет в `ids`, остаются в конце в прежнем порядке.
    pub fn reorder(&mut self, ids: &[u64]) {
        let mut rest = std::mem::take(&mut self.jobs);
        let mut out = Vec::with_capacity(rest.len());
        for id in ids {
            if let Some(i) = rest.iter().position(|j| j.id == *id) {
                out.push(rest.remove(i));
            }
        }
        out.extend(rest);
        self.jobs = out;
        self.sync();
    }

    pub fn remove(&mut self, id: u64) {
        self.jobs.retain(|j| j.id != id);
    }

    /// Повторить отменённую или упавшую загрузку: скачанное раньше пропустится по архиву.
    pub fn retry(&mut self, id: u64) {
        if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
            j.state = State::Queued;
            j.stage = "queued".into();
            j.error.clear();
            j.notice.clear();
            j.message.clear();
            j.percent = 0.0;
            j.files.clear();
        }
        self.sync();
        self.start();
    }

    /// Убрать законченные карточки.
    pub fn clear_done(&mut self) {
        self.jobs.retain(|j| j.state.is_active());
    }

    /// Остановить всё (выход из программы): очередь пустеет, идущие загрузки отменяются.
    pub fn stop_all(&mut self) {
        let mut sh = self.lock();
        sh.stop_all = true;
        sh.queue.clear();
        for (_, c) in sh.current.clone() {
            c.cancel();
        }
    }

    /// Применить событие от рабочей нити к карточкам.
    pub fn handle(&mut self, ev: Event) -> Outcome {
        match ev {
            Event::Progress(id, p) => {
                if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                    j.state = State::Running;
                    if !p.title.is_empty() {
                        j.title = p.title.clone();
                    }
                    j.stage = p.stage.clone();
                    j.item = p.item.clone();
                    j.percent = p.percent;
                    j.speed = p.speed.clone();
                    j.eta = p.eta.clone();
                    j.message = p.message.clone();
                    if !p.notice.is_empty() {
                        j.notice = p.notice.clone();
                    }
                }
                Outcome::Progress(id)
            }
            Event::Log(m) => Outcome::Log(m),
            Event::Finish(id, state, files, err) => {
                // Остановили ради паузы — карточка «на паузе», а не «отменено».
                let state = if self.pausing.remove(&id) && state == State::Cancelled { State::Paused } else { state };
                if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
                    j.state = state;
                    j.stage = match state {
                        State::Done => "done",
                        State::Error => "error",
                        State::Paused => "paused",
                        _ => "cancel",
                    }
                    .into();
                    j.percent = if state == State::Done { 100.0 } else { 0.0 };
                    j.speed.clear();
                    j.eta.clear();
                    j.item.clear();
                    j.message = if state == State::Done {
                        if files.is_empty() {
                            "новых видео нет — всё уже скачано раньше".into()
                        } else {
                            format!("файлов: {}", files.len())
                        }
                    } else {
                        String::new()
                    };
                    j.files = files;
                    j.error = err.map(|e| e.lines().next().unwrap_or("").chars().take(200).collect()).unwrap_or_default();
                }
                Outcome::Finished(id)
            }
            Event::Idle => {
                // Нить закончила, а в очереди что-то появилось, пока она останавливалась, — запустить снова.
                if self.jobs.iter().any(|j| j.state == State::Queued) {
                    self.start();
                }
                Outcome::Idle
            }
        }
    }

    /// Состояние группы карточек (заявка из браузера: несколько ссылок).
    pub fn ticket_state(&self, ids: &[u64]) -> TicketState {
        let mine: Vec<&Job> = self.jobs.iter().filter(|j| ids.contains(&j.id)).collect();
        if let Some(j) = mine.iter().find(|j| j.state == State::Running) {
            return TicketState::Running(j.percent);
        }
        if mine.iter().any(|j| matches!(j.state, State::Queued | State::Paused)) {
            return TicketState::Queued;
        }
        let files: usize = mine.iter().map(|j| j.files.len()).sum();
        match mine.iter().find(|j| j.state == State::Error) {
            Some(j) if files == 0 => TicketState::Error(j.error.clone()),
            _ if mine.iter().all(|j| j.state == State::Cancelled) => TicketState::Error("отменено".into()),
            _ if files == 0 => TicketState::Done("уже было скачано раньше".into()),
            _ => TicketState::Done(format!("файлов: {files}")),
        }
    }
}

/// Рабочая нить: берёт ссылки из очереди, пока они есть.
fn work(shared: Arc<Mutex<Shared>>, sink: Sink, side: bool) {
    let lock = || shared.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let (id, url, settings, opts) = {
            let mut sh = lock();
            if sh.stop_all {
                break;
            }
            let pos = if side {
                sh.queue.iter().position(|(_, u, _)| crate::music::is_small_music(u))
            } else if sh.queue.is_empty() {
                None
            } else {
                Some(0)
            };
            match pos.and_then(|i| sh.queue.remove(i)) {
                // Общие настройки → пресет → свои значения ссылки.
                Some((id, url, opts)) => {
                    sh.taken.insert(id);
                    (id, url, sh.settings.for_job(&opts), opts)
                }
                None => break,
            }
        };
        let (psink, lsink) = (sink.clone(), sink.clone());
        let mut dl = Downloader::new(
            settings,
            Arc::new(move |p: &Progress| psink(Event::Progress(id, p.clone()))),
            Arc::new(move |m: String| lsink(Event::Log(m))),
        );
        dl.opts = opts;
        lock().current.push((id, dl.cancel_handle()));
        let ev = match dl.run(&url) {
            Ok(files) => Event::Finish(id, State::Done, files, None),
            Err(DlError::Cancelled(m)) => {
                if !m.is_empty() {
                    sink(Event::Log(m.clone()));
                }
                Event::Finish(id, State::Cancelled, Vec::new(), if m.is_empty() { None } else { Some(m) })
            }
            Err(DlError::Failed(m)) => {
                sink(Event::Log(format!("Ошибка ({url}): {m}")));
                Event::Finish(id, State::Error, Vec::new(), Some(m))
            }
        };
        {
            let mut sh = lock();
            sh.current.retain(|(cid, _)| *cid != id);
            sh.taken.remove(&id);
        }
        sink(ev);
    }
    sink(Event::Idle);
}

// ---------- очередь со своей нитью-насосом (Android, консоль) ----------

/// Очередь, которая сама применяет события нитей: хозяину не нужен цикл окна.
/// `listener` вызывается после каждого события (из нити-насоса) — обновить уведомление, экран.
pub struct Runner {
    queue: Arc<Mutex<Queue>>,
}

pub type Listener = Arc<dyn Fn(&Outcome, &Queue) + Send + Sync>;

impl Runner {
    pub fn new(settings: Settings, listener: Listener) -> Self {
        let (tx, rx): (Sender<Event>, Receiver<Event>) = channel();
        let sink: Sink = Arc::new(move |e| {
            let _ = tx.send(e);
        });
        let queue = Arc::new(Mutex::new(Queue::new(settings, sink)));
        let q = queue.clone();
        std::thread::spawn(move || {
            for ev in rx {
                let mut g = q.lock().unwrap_or_else(|e| e.into_inner());
                let out = g.handle(ev);
                listener(&out, &g);
            }
        });
        Runner { queue }
    }

    /// Доступ к очереди (добавить, пауза, порядок…). Замок держать недолго.
    pub fn with<T>(&self, f: impl FnOnce(&mut Queue) -> T) -> T {
        let mut g = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut g)
    }

    /// Карточки (копия).
    pub fn jobs(&self) -> Vec<Job> {
        self.with(|q| q.jobs.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet() -> Queue {
        Queue::new(Settings::default(), Arc::new(|_| {}))
    }

    fn queued_urls(q: &Queue) -> Vec<String> {
        q.shared.lock().unwrap().queue.iter().map(|(_, u, _)| u.clone()).collect()
    }

    #[test]
    fn order_pause_resume_move() {
        let mut q = quiet();
        let a = q.add("https://a".into(), JobOpts::default(), None, None);
        let b = q.add("https://b".into(), JobOpts::default(), None, Some("Бэ".into()));
        let c = q.add("https://c".into(), JobOpts::default(), None, None);
        assert_eq!(q.job(b).unwrap().title, "Бэ");
        assert_eq!(q.job(a).unwrap().title, "https://a");
        assert_eq!(queued_urls(&q), ["https://a", "https://b", "https://c"]);
        q.move_job(c, -1);
        assert_eq!(queued_urls(&q), ["https://a", "https://c", "https://b"]);
        q.pause(a);
        assert_eq!(q.job(a).unwrap().state, State::Paused);
        assert_eq!(queued_urls(&q), ["https://c", "https://b"]);
        q.cancel(b);
        assert_eq!(q.job(b).unwrap().state, State::Cancelled);
        assert_eq!(queued_urls(&q), ["https://c"]);
        q.clear_done();
        assert!(q.job(b).is_none());
        q.move_job(a, 5); // за край — ничего
        assert_eq!(q.jobs.len(), 2);
    }

    #[test]
    fn finish_after_pause_is_paused_not_cancelled() {
        let mut q = quiet();
        let a = q.add("https://a".into(), JobOpts::default(), None, None);
        q.handle(Event::Progress(a, Progress { stage: "video".into(), percent: 40.0, title: "Видео".into(), ..Default::default() }));
        assert_eq!(q.job(a).unwrap().state, State::Running);
        assert_eq!(q.job(a).unwrap().title, "Видео");
        q.pause(a);
        assert_eq!(q.handle(Event::Finish(a, State::Cancelled, vec![], None)), Outcome::Finished(a));
        assert_eq!(q.job(a).unwrap().state, State::Paused);
        assert_eq!(q.job(a).unwrap().stage, "paused");
        // а обычная отмена — «отменено»
        let b = q.add("https://b".into(), JobOpts::default(), None, None);
        q.handle(Event::Progress(b, Progress::default()));
        q.handle(Event::Finish(b, State::Cancelled, vec![], None));
        assert_eq!(q.job(b).unwrap().state, State::Cancelled);
    }

    #[test]
    fn finish_message_and_tickets() {
        let mut q = quiet();
        let a = q.add("https://a".into(), JobOpts::default(), None, None);
        let b = q.add("https://b".into(), JobOpts::default(), None, None);
        assert_eq!(q.ticket_state(&[a, b]), TicketState::Queued);
        q.handle(Event::Progress(a, Progress { percent: 55.0, ..Default::default() }));
        assert_eq!(q.ticket_state(&[a, b]), TicketState::Running(55.0));
        q.handle(Event::Finish(a, State::Done, vec!["x.mkv".into()], None));
        assert_eq!(q.job(a).unwrap().message, "файлов: 1");
        assert_eq!(q.job(a).unwrap().percent, 100.0);
        q.handle(Event::Finish(b, State::Error, vec![], Some("HTTP 403\nподробности".into())));
        assert_eq!(q.job(b).unwrap().error, "HTTP 403");
        assert_eq!(q.ticket_state(&[a, b]), TicketState::Done("файлов: 1".into()));
        assert_eq!(q.ticket_state(&[b]), TicketState::Error("HTTP 403".into()));
        q.retry(b);
        q.stop_all();
        assert_eq!(q.job(b).unwrap().state, State::Queued);
        assert!(queued_urls(&q).is_empty(), "stop_all чистит очередь нитей");
    }

    /// Взятая нитью ссылка не ставится в очередь второй раз, пока карточка ещё «в очереди».
    #[test]
    fn taken_job_is_not_queued_twice() {
        let mut q = quiet();
        let a = q.add("https://a".into(), JobOpts::default(), None, None);
        // нить взяла «a» (как в `work`), первое событие прогресса ещё не дошло
        {
            let mut sh = q.shared.lock().unwrap();
            sh.queue.pop_front();
            sh.taken.insert(a);
        }
        q.add("https://b".into(), JobOpts::default(), None, None); // sync
        assert_eq!(queued_urls(&q), ["https://b"]);
        q.shared.lock().unwrap().taken.remove(&a);
    }

    /// Приоритет: ↑ у ждущей карточки обгоняет ближайшую ждущую, мимо идущей и готовой.
    #[test]
    fn move_skips_running_and_done() {
        let mut q = quiet();
        let a = q.add("https://a".into(), JobOpts::default(), None, None);
        let b = q.add("https://b".into(), JobOpts::default(), None, None);
        let c = q.add("https://c".into(), JobOpts::default(), None, None);
        let d = q.add("https://d".into(), JobOpts::default(), None, None);
        q.handle(Event::Progress(a, Progress::default())); // a качается
        q.handle(Event::Progress(b, Progress::default()));
        q.handle(Event::Finish(b, State::Done, vec![], None)); // b готова
        q.sync(); // в жизни a и b уже взяли нити
        assert_eq!(queued_urls(&q), ["https://c", "https://d"]);
        q.move_job(d, -1);
        assert_eq!(queued_urls(&q), ["https://d", "https://c"]);
        q.move_job(d, -1); // выше ждущих нет — на месте
        assert_eq!(queued_urls(&q), ["https://d", "https://c"]);
        q.move_job(a, 1); // идущую не двигаем
        assert_eq!(q.jobs[0].id, a);
        // перетаскивание
        q.reorder(&[c, a, d, 999]);
        assert_eq!(q.jobs.iter().map(|j| j.id).collect::<Vec<_>>(), [c, a, d, b]);
        assert_eq!(queued_urls(&q), ["https://c", "https://d"]);
    }

    #[test]
    fn pause_all_pauses_queued() {
        let mut q = quiet();
        let a = q.add("https://a".into(), JobOpts::default(), None, None);
        let b = q.add("https://b".into(), JobOpts::default(), None, None);
        q.pause_all();
        assert!([a, b].iter().all(|id| q.job(*id).unwrap().state == State::Paused));
        assert!(queued_urls(&q).is_empty());
    }
}
