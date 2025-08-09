// データーベースから測定値を得る。
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use anyhow::Context;
use chrono::{DateTime, Utc};
use chrono_tz::Asia;
use rust_decimal::Decimal;
use sqlx::{self, postgres::PgPool};
use std::env;
use std::result;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv()?;

    // 環境変数
    let database_url = env::var("DATABASE_URL").context("Must be set to DATABASE_URL")?;

    let pool = PgPool::connect(&database_url).await?;

    //
    let xs = read_instant_epower(&pool).await?;
    println!("time, instantious electric power(W)");
    for (at, power) in xs.iter() {
        let t = at.with_timezone(&Asia::Tokyo).to_rfc3339();
        println!("{t}, {power}");
    }
    println!("");

    let xs = read_instant_current(&pool).await?;
    println!("time, instantious current R(A), T(A)");
    for (at, ir, it) in xs.iter() {
        let t = at.with_timezone(&Asia::Tokyo).to_rfc3339();
        println!(
            "{t}, {ir}{}",
            it.map(|v| format!(", {v}")).unwrap_or_default()
        );
    }
    println!("");

    let xs = read_cumlative_amount_epower(&pool).await?;
    println!("time, cumlative amounts of power(kWh)");
    for (at, power) in xs.iter() {
        let t = at.with_timezone(&Asia::Tokyo).to_rfc3339();
        println!("{t}, {power}");
    }
    println!("");

    Ok(())
}

/// 瞬時電力をデーターベースから得る
async fn read_instant_epower(
    pool: &PgPool,
) -> result::Result<Vec<(DateTime<Utc>, Decimal)>, sqlx::Error> {
    let mut recs = sqlx::query!(
        "SELECT recorded_at, watt FROM instant_epower ORDER BY recorded_at DESC LIMIT $1",
        100
    )
    .fetch_all(pool)
    .await?;

    recs.reverse();
    Ok(recs.iter().map(|a| (a.recorded_at, a.watt)).collect())
}

/// 瞬時電力をデーターベースから得る
async fn read_instant_current(
    pool: &PgPool,
) -> result::Result<Vec<(DateTime<Utc>, Decimal, Option<Decimal>)>, sqlx::Error> {
    let mut recs = sqlx::query!(
        "SELECT recorded_at, r, t FROM instant_current ORDER BY recorded_at DESC LIMIT $1",
        100
    )
    .fetch_all(pool)
    .await?;

    recs.reverse();
    Ok(recs.iter().map(|a| (a.recorded_at, a.r, a.t)).collect())
}

/// 定時積算電力量計測値(正方向計測値)をデーターベースに蓄積する
async fn read_cumlative_amount_epower(
    pool: &PgPool,
) -> result::Result<Vec<(DateTime<Utc>, Decimal)>, sqlx::Error> {
    let mut recs = sqlx::query!(
        "SELECT recorded_at, kwh FROM cumlative_amount_epower ORDER BY recorded_at DESC LIMIT $1",
        100
    )
    .fetch_all(pool)
    .await?;

    recs.reverse();
    Ok(recs.iter().map(|a| (a.recorded_at, a.kwh)).collect())
}
