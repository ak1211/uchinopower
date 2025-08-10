//! # [Ratatui] `BarChart` example
//!
//! The latest version of this example is available in the [examples] folder in the repository.
//!
//! Please note that the examples are designed to be run against the `main` branch of the Github
//! repository. This means that you may not be able to compile with the latest release version on
//! crates.io, or the one that you have installed locally.
//!
//! See the [examples readme] for more information on finding examples that match the version of the
//! library you are using.
//!
//! [Ratatui]: https://github.com/ratatui/ratatui
//! [examples]: https://github.com/ratatui/ratatui/blob/main/examples
//! [examples readme]: https://github.com/ratatui/ratatui/blob/main/examples/README.md

use chrono::{DateTime, Utc};
use color_eyre::{Result, eyre::Context};
use futures::StreamExt;
use hsv;
use ratatui::widgets::Block;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{Event, EventStream, KeyCode, KeyEventKind},
    layout::{Constraint, Layout},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Bar, BarChart, BarGroup},
};
use rust_decimal::Decimal;
use sqlx::{self, postgres::PgPool};
use std::env;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let database_url = env::var("DATABASE_URL").wrap_err("Must be set to DATABASE_URL")?;
    let pool = PgPool::connect(&database_url).await?;
    color_eyre::install()?;
    let terminal = ratatui::init();
    let app = App::new(pool).await;
    let app_result = app.run(terminal).await;
    ratatui::restore();
    app_result
}

struct InstantWatt {
    pub recorded_at: DateTime<Utc>,
    pub watt: Decimal,
}

#[allow(dead_code)]
struct InstantCurrent {
    pub recorded_at: DateTime<Utc>,
    pub r: Decimal,
    pub t: Option<Decimal>,
}

#[allow(dead_code)]
struct CumlativeKiloWattHour {
    pub recorded_at: DateTime<Utc>,
    pub kwh: Decimal,
}

struct App {
    pool: PgPool,
    should_quit: bool,
    instant_watt: Vec<InstantWatt>,
    instant_current: Vec<InstantCurrent>,
    cumlative_amount_epower: Vec<CumlativeKiloWattHour>,
}

impl App {
    const FRAMES_PER_SECOND: f32 = 60.0;

    async fn new(pool: PgPool) -> Self {
        let instant_watt = read_instant_epower(&pool).await.unwrap_or_default();
        let instant_current = read_instant_current(&pool).await.unwrap_or_default();
        let cumlative_amount_epower = read_cumlative_amount_epower(&pool)
            .await
            .unwrap_or_default();

        Self {
            pool: pool,
            should_quit: false,
            instant_watt,
            instant_current,
            cumlative_amount_epower,
        }
    }

    async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let period = Duration::from_secs_f32(1.0 / Self::FRAMES_PER_SECOND);
        let mut interval = tokio::time::interval(period);
        let mut fetch_interval = tokio::time::interval(Duration::from_secs(10));
        let mut events = EventStream::new();

        while !self.should_quit {
            tokio::select! {
                _ = interval.tick() => { terminal.draw(|frame| self.draw(frame))?; },
                _ = fetch_interval.tick() => self.fetch_data().await.unwrap_or_default(),
                Some(Ok(event)) = events.next() => self.handle_event(&event),
            }
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        let [title, upper, lower] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ])
        .spacing(1)
        .areas(frame.area());
        let now = Utc::now();

        frame.render_widget(
            "DASHBOARD (press q key to exit.)"
                .bold()
                .into_centered_line(),
            title,
        );
        frame.render_widget(
            cumlative_amount_epower_chart(now, &self.cumlative_amount_epower),
            upper,
        );
        frame.render_widget(instantious_watt_chart(now, &self.instant_watt), lower);
    }

    fn handle_event(&mut self, event: &Event) {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                self.should_quit = true;
            }
        }
    }

    async fn fetch_data(&mut self) -> Result<()> {
        self.instant_watt = read_instant_epower(&self.pool).await?;
        self.instant_current = read_instant_current(&self.pool).await?;
        self.cumlative_amount_epower = read_cumlative_amount_epower(&self.pool).await?;
        Ok(())
    }
}

/// 瞬時電力をデーターベースから得る
async fn read_instant_epower(pool: &PgPool) -> Result<Vec<InstantWatt>> {
    let mut recs = sqlx::query!(
        "SELECT recorded_at, watt FROM instant_epower ORDER BY recorded_at DESC LIMIT $1",
        20
    )
    .fetch_all(pool)
    .await?;

    recs.reverse();
    Ok(recs
        .iter()
        .map(|a| InstantWatt {
            recorded_at: a.recorded_at,
            watt: a.watt,
        })
        .collect())
}

/// 瞬時電流をデーターベースから得る
async fn read_instant_current(pool: &PgPool) -> Result<Vec<InstantCurrent>> {
    let mut recs = sqlx::query!(
        "SELECT recorded_at, r, t FROM instant_current ORDER BY recorded_at DESC LIMIT $1",
        20
    )
    .fetch_all(pool)
    .await?;

    recs.reverse();
    Ok(recs
        .iter()
        .map(|a| InstantCurrent {
            recorded_at: a.recorded_at,
            r: a.r,
            t: a.t,
        })
        .collect())
}

/// 定時積算電力量計測値(正方向計測値)をデーターベースから得る
async fn read_cumlative_amount_epower(pool: &PgPool) -> Result<Vec<CumlativeKiloWattHour>> {
    let mut recs = sqlx::query!(
        "SELECT recorded_at, kwh FROM cumlative_amount_epower ORDER BY recorded_at DESC LIMIT $1",
        10
    )
    .fetch_all(pool)
    .await?;

    recs.reverse();
    Ok(recs
        .iter()
        .map(|a| CumlativeKiloWattHour {
            recorded_at: a.recorded_at,
            kwh: a.kwh,
        })
        .collect())
}

fn instantious_watt_chart(now: DateTime<Utc>, iw: &[InstantWatt]) -> BarChart {
    let bars: Vec<Bar> = iw
        .iter()
        .map(|a| {
            let diff_minutes = (now - a.recorded_at).num_seconds() as f64 / 60.0;
            let value: u32 = a.watt.try_into().unwrap();
            let (r, g, b) = hsv::hsv_to_rgb(60.0, 1.0, 1.0 - f64::from(value) / 5000.0);
            let style = Style::new().fg(Color::Rgb(r, g, b));
            //
            Bar::default()
                .value(value as u64)
                .label(Line::from(format!("{}m", 0.0 - diff_minutes.floor())))
                .text_value(format!("{value:>3}"))
                .style(style)
                .value_style(style.reversed())
        })
        .collect();
    let title = Line::from("instantious electric power (W)").centered();
    BarChart::default()
        .data(BarGroup::default().bars(&bars))
        .block(Block::new().title(title))
        .bar_width(5)
        .bar_gap(1)
}

fn cumlative_amount_epower_chart(now: DateTime<Utc>, kwh: &[CumlativeKiloWattHour]) -> BarChart {
    let bars: Vec<Bar> = kwh
        .iter()
        .map(|a| {
            let diff_minutes = (now - a.recorded_at).num_seconds() as f64 / 60.0;
            let value: f64 = a.kwh.try_into().unwrap();
            let (r, g, b) = hsv::hsv_to_rgb(180.0, 1.0, 1.0 - value / 99999.0);
            let style = Style::new().fg(Color::Rgb(r, g, b));
            //
            Bar::default()
                .value((a.kwh * Decimal::from(100)).try_into().unwrap())
                .label(Line::from(format!("{}m", 0.0 - diff_minutes.floor())))
                .text_value(format!("{value}"))
                .style(style)
                .value_style(style.reversed())
        })
        .collect();
    let title = Line::from("cumlative amount electric power (kWh)").centered();
    BarChart::default()
        .data(BarGroup::default().bars(&bars))
        .block(Block::new().title(title))
        .bar_width(10)
        .bar_gap(2)
}
