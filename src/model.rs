//! Temporizadores, alarmas y predefinidos. Sin GPUI, para poder probarlo.
//!
//! Los instantes son milisegundos desde 1970 (UTC). Se usa la hora del reloj y no
//! un `Instant`, para que un temporizador sobreviva a un reinicio de la app y para
//! que la suspensión del equipo no lo "congele".

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Timelike};
use serde::{Deserialize, Serialize};

pub type Millis = i64;

pub const SECOND: Millis = 1000;
pub const MINUTE: Millis = 60 * SECOND;
pub const HOUR: Millis = 60 * MINUTE;

/// Una alarma o temporizador vencido hace más que esto (la app estaba cerrada o el
/// equipo suspendido) ya no suena: se reprograma o se da por perdido.
pub const STALE: Millis = HOUR;

pub fn now_ms() -> Millis {
    Local::now().timestamp_millis()
}

pub fn to_local(ms: Millis) -> DateTime<Local> {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Local::now)
}

/// Qué está sonando (o puede sonar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemRef {
    Timer(u64),
    Alarm(u64),
}

impl ItemRef {
    /// Texto para los botones de las notificaciones: `timer:3`, `alarm:7`.
    pub fn encode(self) -> String {
        match self {
            Self::Timer(id) => format!("timer:{id}"),
            Self::Alarm(id) => format!("alarm:{id}"),
        }
    }

    pub fn decode(s: &str) -> Option<Self> {
        let (kind, id) = s.split_once(':')?;
        let id = id.parse().ok()?;
        match kind {
            "timer" => Some(Self::Timer(id)),
            "alarm" => Some(Self::Alarm(id)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preset {
    pub label: String,
    pub secs: u64,
}

impl Preset {
    pub fn new(label: impl Into<String>, secs: u64) -> Self {
        Self {
            label: label.into(),
            secs,
        }
    }

    /// Texto del botón: el nombre, o la duración si no tiene.
    pub fn title(&self) -> String {
        if self.label.trim().is_empty() {
            short_duration(self.secs as Millis * SECOND)
        } else {
            self.label.clone()
        }
    }

    pub fn defaults() -> Vec<Self> {
        vec![
            Self::new("1 min", 60),
            Self::new("3 min", 3 * 60),
            Self::new("5 min", 5 * 60),
            Self::new("10 min", 10 * 60),
            Self::new("15 min", 15 * 60),
            Self::new("Pomodoro", 25 * 60),
            Self::new("30 min", 30 * 60),
            Self::new("1 h", 60 * 60),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TimerState {
    /// Detenido con `remaining_ms` por delante. Recién creado o reiniciado, es la
    /// duración completa.
    Paused {
        remaining_ms: Millis,
    },
    Running {
        ends_at: Millis,
    },
    /// Llegó a cero; sigue así hasta que se detenga o se le sume tiempo.
    Ringing {
        since: Millis,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timer {
    pub id: u64,
    pub label: String,
    pub duration_ms: Millis,
    pub state: TimerState,
}

impl Timer {
    pub fn new(id: u64, label: String, duration_ms: Millis, now: Millis) -> Self {
        Self {
            id,
            label,
            duration_ms,
            state: TimerState::Running {
                ends_at: now + duration_ms,
            },
        }
    }

    pub fn title(&self) -> String {
        if self.label.trim().is_empty() {
            format!("Temporizador de {}", short_duration(self.duration_ms))
        } else {
            self.label.clone()
        }
    }

    /// Para listas chicas: el nombre, o solo la duración ("5 min").
    pub fn short_title(&self) -> String {
        if self.label.trim().is_empty() {
            short_duration(self.duration_ms)
        } else {
            self.label.clone()
        }
    }

    pub fn remaining(&self, now: Millis) -> Millis {
        match self.state {
            TimerState::Paused { remaining_ms } => remaining_ms,
            TimerState::Running { ends_at } => (ends_at - now).max(0),
            TimerState::Ringing { .. } => 0,
        }
    }

    /// Fracción transcurrida, de 0 a 1.
    pub fn progress(&self, now: Millis) -> f32 {
        if self.duration_ms <= 0 {
            return 1.0;
        }
        let total = self.duration_ms.max(self.remaining(now));
        1.0 - self.remaining(now) as f32 / total as f32
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, TimerState::Running { .. })
    }

    pub fn is_ringing(&self) -> bool {
        matches!(self.state, TimerState::Ringing { .. })
    }

    /// Está como recién creado: detenido y con la duración completa.
    pub fn is_reset(&self) -> bool {
        self.state
            == TimerState::Paused {
                remaining_ms: self.duration_ms,
            }
    }

    pub fn ends_at(&self) -> Option<Millis> {
        match self.state {
            TimerState::Running { ends_at } => Some(ends_at),
            _ => None,
        }
    }

    pub fn toggle(&mut self, now: Millis) {
        self.state = match self.state {
            TimerState::Running { ends_at } => TimerState::Paused {
                remaining_ms: (ends_at - now).max(0),
            },
            TimerState::Paused { remaining_ms } if remaining_ms > 0 => TimerState::Running {
                ends_at: now + remaining_ms,
            },
            // Pausado en cero o sonando: vuelve a empezar.
            _ => TimerState::Running {
                ends_at: now + self.duration_ms,
            },
        };
    }

    #[cfg(test)]
    pub fn reset(&mut self) {
        self.state = TimerState::Paused {
            remaining_ms: self.duration_ms,
        };
    }

    /// Suma tiempo; si estaba sonando, vuelve a correr con ese tiempo.
    pub fn add(&mut self, ms: Millis, now: Millis) {
        self.state = match self.state {
            TimerState::Running { ends_at } => TimerState::Running {
                ends_at: ends_at + ms,
            },
            TimerState::Paused { remaining_ms } => TimerState::Paused {
                remaining_ms: remaining_ms + ms,
            },
            TimerState::Ringing { .. } => TimerState::Running { ends_at: now + ms },
        };
    }
}

/// Días de la semana como máscara de bits: bit 0 = lunes … bit 6 = domingo.
/// `0` es "una sola vez".
pub type Days = u8;

pub const ALL_DAYS: Days = 0b111_1111;
pub const WEEKDAYS: Days = 0b001_1111;
pub const WEEKEND: Days = 0b110_0000;
pub const DAY_LETTERS: [&str; 7] = ["L", "M", "X", "J", "V", "S", "D"];
pub const DAY_NAMES: [&str; 7] = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"];

pub fn days_summary(days: Days) -> String {
    match days & ALL_DAYS {
        0 => "Una vez".into(),
        ALL_DAYS => "Todos los días".into(),
        WEEKDAYS => "Lunes a viernes".into(),
        WEEKEND => "Fines de semana".into(),
        d => (0..7)
            .filter(|i| d & (1 << i) != 0)
            .map(|i| DAY_NAMES[i])
            .collect::<Vec<_>>()
            .join(", "),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alarm {
    pub id: u64,
    pub label: String,
    pub hour: u32,
    pub minute: u32,
    #[serde(default)]
    pub days: Days,
    pub enabled: bool,
    /// Próxima vez que suena; `None` si está apagada.
    pub next_at: Option<Millis>,
    /// `next_at` es una posposición, no la hora programada.
    #[serde(default)]
    pub snoozed: bool,
    /// Sonando desde este instante, hasta que se detenga o se posponga.
    #[serde(default)]
    pub ringing_since: Option<Millis>,
}

impl Alarm {
    pub fn new(id: u64, label: String, hour: u32, minute: u32, days: Days, now: Millis) -> Self {
        let mut alarm = Self {
            id,
            label,
            hour,
            minute,
            days,
            enabled: true,
            next_at: None,
            snoozed: false,
            ringing_since: None,
        };
        alarm.schedule(now);
        alarm
    }

    pub fn time_text(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    pub fn title(&self) -> String {
        if self.label.trim().is_empty() {
            format!("Alarma de las {}", self.time_text())
        } else {
            self.label.clone()
        }
    }

    /// Programa la próxima vez que toca, a partir de `now`.
    pub fn schedule(&mut self, now: Millis) {
        self.snoozed = false;
        self.next_at = self
            .enabled
            .then(|| next_occurrence(self.hour, self.minute, self.days, to_local(now)))
            .map(|t| t.timestamp_millis());
    }

    pub fn set_enabled(&mut self, enabled: bool, now: Millis) {
        self.enabled = enabled;
        if !enabled {
            self.ringing_since = None;
        }
        self.schedule(now);
    }

    /// Se detuvo después de sonar: la que es de una vez se apaga, la que se repite
    /// pasa al próximo día que le toca.
    pub fn dismiss(&mut self, now: Millis) {
        self.ringing_since = None;
        if self.days == 0 {
            self.enabled = false;
        }
        self.schedule(now);
    }

    pub fn snooze(&mut self, ms: Millis, now: Millis) {
        self.ringing_since = None;
        self.enabled = true;
        self.next_at = Some(now + ms);
        self.snoozed = true;
    }
}

/// La próxima fecha y hora, estrictamente posterior a `after`, en que cae `hour:minute`
/// en uno de los días de `days` (cualquier día si es `0`).
pub fn next_occurrence(
    hour: u32,
    minute: u32,
    days: Days,
    after: DateTime<Local>,
) -> DateTime<Local> {
    let time = NaiveTime::from_hms_opt(hour.min(23), minute.min(59), 0).unwrap_or_default();
    let today = after.date_naive();
    for offset in 0..=8 {
        let date = today + Duration::days(offset);
        let weekday = date.weekday().num_days_from_monday();
        if days != 0 && days & (1 << weekday) == 0 {
            continue;
        }
        // En el cambio de hora una hora local puede no existir (se salta) o repetirse.
        let Some(candidate) = Local.from_local_datetime(&date.and_time(time)).earliest() else {
            continue;
        };
        if candidate > after {
            return candidate;
        }
    }
    after + Duration::days(1)
}

/// Lo que empezó a sonar en un `tick`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fired {
    pub item: ItemRef,
    pub title: String,
    pub detail: String,
}

/// Todo lo que se guarda en `state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub timers: Vec<Timer>,
    pub alarms: Vec<Alarm>,
    pub presets: Vec<Preset>,
    pub next_id: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            timers: Vec::new(),
            alarms: Vec::new(),
            presets: Preset::defaults(),
            next_id: 1,
        }
    }
}

impl State {
    pub fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn timer_mut(&mut self, id: u64) -> Option<&mut Timer> {
        self.timers.iter_mut().find(|t| t.id == id)
    }

    pub fn alarm_mut(&mut self, id: u64) -> Option<&mut Alarm> {
        self.alarms.iter_mut().find(|a| a.id == id)
    }

    /// Lo que está sonando, con el instante en que empezó.
    pub fn ringing(&self) -> Vec<(ItemRef, Millis)> {
        let timers = self.timers.iter().filter_map(|t| match t.state {
            TimerState::Ringing { since } => Some((ItemRef::Timer(t.id), since)),
            _ => None,
        });
        let alarms = self
            .alarms
            .iter()
            .filter_map(|a| a.ringing_since.map(|since| (ItemRef::Alarm(a.id), since)));
        let mut all: Vec<_> = timers.chain(alarms).collect();
        all.sort_by_key(|(_, since)| *since);
        all
    }

    /// Pasa a "sonando" lo que venció. Devuelve lo que empezó a sonar ahora.
    pub fn tick(&mut self, now: Millis) -> Vec<Fired> {
        let mut fired = Vec::new();
        for timer in &mut self.timers {
            if let TimerState::Running { ends_at } = timer.state
                && ends_at <= now
            {
                timer.state = TimerState::Ringing { since: ends_at };
                let late = now - ends_at > 5 * SECOND;
                fired.push(Fired {
                    item: ItemRef::Timer(timer.id),
                    title: timer.title(),
                    detail: if late {
                        format!("Terminó a las {}", to_local(ends_at).format("%H:%M"))
                    } else {
                        format!("Pasaron {}", short_duration(timer.duration_ms))
                    },
                });
            }
        }
        for alarm in &mut self.alarms {
            let Some(at) = alarm.next_at.filter(|at| *at <= now) else {
                continue;
            };
            if alarm.ringing_since.is_some() {
                continue;
            }
            if now - at > STALE {
                // Se perdió (app cerrada o equipo suspendido): no suena a destiempo.
                alarm.dismiss(now);
                continue;
            }
            alarm.ringing_since = Some(at);
            alarm.next_at = None;
            fired.push(Fired {
                item: ItemRef::Alarm(alarm.id),
                title: alarm.title(),
                detail: if alarm.snoozed {
                    format!("Pospuesta · alarma de las {}", alarm.time_text())
                } else {
                    format!("Son las {}", alarm.time_text())
                },
            });
        }
        fired
    }

    /// Detiene lo que suena. Un temporizador queda listo para volver a empezar.
    pub fn stop(&mut self, item: ItemRef, now: Millis) {
        match item {
            // Un temporizador que terminó y se detiene ya cumplió: se quita.
            ItemRef::Timer(id) => self.timers.retain(|t| t.id != id || !t.is_ringing()),
            ItemRef::Alarm(id) => {
                if let Some(a) = self.alarm_mut(id)
                    && a.ringing_since.is_some()
                {
                    a.dismiss(now);
                }
            }
        }
    }

    pub fn snooze(&mut self, item: ItemRef, ms: Millis, now: Millis) {
        match item {
            ItemRef::Timer(id) => {
                if let Some(t) = self.timer_mut(id) {
                    t.add(ms, now);
                }
            }
            ItemRef::Alarm(id) => {
                if let Some(a) = self.alarm_mut(id) {
                    a.snooze(ms, now);
                }
            }
        }
    }

    /// Título y hora de inicio de algo que está sonando, para mostrarlo.
    pub fn describe(&self, item: ItemRef) -> Option<String> {
        match item {
            ItemRef::Timer(id) => self.timers.iter().find(|t| t.id == id).map(Timer::title),
            ItemRef::Alarm(id) => self.alarms.iter().find(|a| a.id == id).map(Alarm::title),
        }
    }

    /// El temporizador en marcha que termina antes.
    pub fn soonest_timer(&self) -> Option<&Timer> {
        self.timers
            .iter()
            .filter(|t| t.is_running())
            .min_by_key(|t| t.ends_at())
    }
}

/// `1:05:09`, `05:09` o `00:42`. Redondea hacia arriba: muestra `00:01` hasta llegar a cero.
pub fn clock(ms: Millis) -> String {
    let secs = (ms.max(0) + SECOND - 1) / SECOND;
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// `1 h 30 min`, `25 min`, `45 s`, `2 min 30 s`.
pub fn short_duration(ms: Millis) -> String {
    let secs = (ms.max(0) + SECOND - 1) / SECOND;
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    let mut parts = Vec::new();
    if h > 0 {
        parts.push(format!("{h} h"));
    }
    if m > 0 {
        parts.push(format!("{m} min"));
    }
    if s > 0 && h == 0 {
        parts.push(format!("{s} s"));
    }
    if parts.is_empty() {
        "0 s".into()
    } else {
        parts.join(" ")
    }
}

/// "en 5 h 12 min", "en 3 min", "en menos de un minuto".
pub fn until(ms: Millis) -> String {
    let mins = (ms.max(0) + MINUTE - 1) / MINUTE;
    match mins {
        0 | 1 if ms < MINUTE => "en menos de un minuto".into(),
        m if m < 60 => format!("en {m} min"),
        m => {
            let (h, m) = (m / 60, m % 60);
            if h >= 24 {
                let d = h / 24;
                let h = h % 24;
                if h == 0 {
                    format!("en {d} d")
                } else {
                    format!("en {d} d {h} h")
                }
            } else if m == 0 {
                format!("en {h} h")
            } else {
                format!("en {h} h {m} min")
            }
        }
    }
}

/// Fecha relativa corta de un instante futuro: "hoy 07:30", "mañana 07:30", "lun 07:30".
pub fn when(at: Millis, now: Millis) -> String {
    let (at, now) = (to_local(at), to_local(now));
    let days = (at.date_naive() - now.date_naive()).num_days();
    let time = format!("{:02}:{:02}", at.hour(), at.minute());
    match days {
        0 => format!("hoy {time}"),
        1 => format!("mañana {time}"),
        _ => format!(
            "{} {time}",
            DAY_NAMES[at.weekday().num_days_from_monday() as usize]
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn reloj_redondea_hacia_arriba() {
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(1), "00:01");
        assert_eq!(clock(59_001), "01:00");
        assert_eq!(clock(5 * MINUTE), "05:00");
        assert_eq!(clock(HOUR + 5 * MINUTE + 9 * SECOND), "1:05:09");
    }

    #[test]
    fn duraciones_cortas() {
        assert_eq!(short_duration(25 * MINUTE), "25 min");
        assert_eq!(short_duration(HOUR + 30 * MINUTE), "1 h 30 min");
        assert_eq!(short_duration(150 * SECOND), "2 min 30 s");
        assert_eq!(short_duration(45 * SECOND), "45 s");
        assert_eq!(until(30 * SECOND), "en menos de un minuto");
        assert_eq!(until(3 * MINUTE), "en 3 min");
        assert_eq!(until(5 * HOUR + 12 * MINUTE), "en 5 h 12 min");
    }

    #[test]
    fn temporizador_pausa_y_sigue() {
        let mut t = Timer::new(1, String::new(), 10 * SECOND, 0);
        assert_eq!(t.remaining(4 * SECOND), 6 * SECOND);
        t.toggle(4 * SECOND);
        assert_eq!(t.remaining(100 * SECOND), 6 * SECOND);
        t.toggle(100 * SECOND);
        assert_eq!(t.ends_at(), Some(106 * SECOND));
        t.add(MINUTE, 100 * SECOND);
        assert_eq!(t.ends_at(), Some(166 * SECOND));
        t.reset();
        assert!(t.is_reset());
    }

    #[test]
    fn tick_hace_sonar_y_detener_lo_quita() {
        let mut s = State::default();
        let id = s.take_id();
        s.timers.push(Timer::new(id, "Té".into(), 3 * MINUTE, 0));
        assert!(s.tick(MINUTE).is_empty());
        let fired = s.tick(3 * MINUTE);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].item, ItemRef::Timer(id));
        // No vuelve a disparar mientras suena.
        assert!(s.tick(4 * MINUTE).is_empty());
        assert_eq!(s.ringing().len(), 1);
        s.stop(ItemRef::Timer(id), 4 * MINUTE);
        assert!(s.ringing().is_empty());
        assert!(s.timers.is_empty());
    }

    #[test]
    fn proxima_ocurrencia() {
        // Sábado 7 de noviembre de 2026, 10:00.
        let sat = local(2026, 11, 7, 10, 0);
        assert_eq!(next_occurrence(11, 0, 0, sat), local(2026, 11, 7, 11, 0));
        assert_eq!(next_occurrence(9, 0, 0, sat), local(2026, 11, 8, 9, 0));
        // Misma hora exacta: la siguiente, no ahora.
        assert_eq!(next_occurrence(10, 0, 0, sat), local(2026, 11, 8, 10, 0));
        // Días de semana: el próximo lunes.
        assert_eq!(
            next_occurrence(7, 30, WEEKDAYS, sat),
            local(2026, 11, 9, 7, 30)
        );
        // Solo sábados, ya pasó hoy: dentro de una semana.
        assert_eq!(
            next_occurrence(9, 0, 1 << 5, sat),
            local(2026, 11, 14, 9, 0)
        );
    }

    #[test]
    fn alarma_de_una_vez_se_apaga() {
        let now = local(2026, 11, 7, 10, 0).timestamp_millis();
        let mut s = State::default();
        let id = s.take_id();
        s.alarms.push(Alarm::new(id, String::new(), 10, 30, 0, now));
        let at = s.alarms[0].next_at.unwrap();
        assert_eq!(at - now, 30 * MINUTE);
        assert_eq!(s.tick(at).len(), 1);
        s.snooze(ItemRef::Alarm(id), 5 * MINUTE, at + SECOND);
        assert!(s.ringing().is_empty());
        assert_eq!(s.tick(at + 5 * MINUTE + SECOND).len(), 1);
        s.stop(ItemRef::Alarm(id), at + 6 * MINUTE);
        assert!(!s.alarms[0].enabled);
        assert_eq!(s.alarms[0].next_at, None);
    }

    #[test]
    fn alarma_perdida_no_suena() {
        let now = local(2026, 11, 7, 10, 0).timestamp_millis();
        let mut s = State::default();
        let id = s.take_id();
        s.alarms
            .push(Alarm::new(id, String::new(), 10, 30, ALL_DAYS, now));
        let at = s.alarms[0].next_at.unwrap();
        assert!(s.tick(at + 2 * HOUR).is_empty());
        assert!(s.alarms[0].enabled);
        assert_eq!(s.alarms[0].next_at, Some(at + 24 * HOUR));
    }

    #[test]
    fn referencias_ida_y_vuelta() {
        for item in [ItemRef::Timer(3), ItemRef::Alarm(12)] {
            assert_eq!(ItemRef::decode(&item.encode()), Some(item));
        }
        assert_eq!(ItemRef::decode("x:1"), None);
    }

    #[test]
    fn resumen_de_dias() {
        assert_eq!(days_summary(0), "Una vez");
        assert_eq!(days_summary(WEEKDAYS), "Lunes a viernes");
        assert_eq!(days_summary(0b101), "lun, mié");
    }
}
