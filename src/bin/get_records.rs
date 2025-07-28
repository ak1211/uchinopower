use anyhow::Context;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{self, postgres::PgPool};
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv()?;

    // 環境変数
    let database_url = env::var("DATABASE_URL").context("Must be set to DATABASE_URL")?;

    let pool = PgPool::connect(&database_url).await?;

    //
    let xs = read_instant_epower(&pool).await?;
    println!("{:?}", xs);

    let xs = read_instant_current(&pool).await?;
    println!("{:?}", xs);

    let xs = read_cumlative_amount_epower(&pool).await?;
    println!("{:?}", xs);

    Ok(())
}

/// 瞬時電力をデーターベースから得る
async fn read_instant_epower(pool: &PgPool) -> anyhow::Result<Vec<(DateTime<Utc>, Decimal)>> {
    let recs = sqlx::query!(
        "SELECT recorded_at, watt FROM instant_epower ORDER BY recorded_at DESC LIMIT $1",
        100
    )
    .fetch_all(pool)
    .await?;

    Ok(recs.iter().map(|a| (a.recorded_at, a.watt)).collect())
}

/// 瞬時電力をデーターベースから得る
async fn read_instant_current(
    pool: &PgPool,
) -> anyhow::Result<Vec<(DateTime<Utc>, Decimal, Option<Decimal>)>> {
    let recs = sqlx::query!(
        "SELECT recorded_at, r, t FROM instant_current ORDER BY recorded_at DESC LIMIT $1",
        100
    )
    .fetch_all(pool)
    .await?;

    Ok(recs.iter().map(|a| (a.recorded_at, a.r, a.t)).collect())
}

/// 定時積算電力量計測値(正方向計測値)をデーターベースに蓄積する
async fn read_cumlative_amount_epower(
    pool: &PgPool,
) -> anyhow::Result<Vec<(DateTime<Utc>, Decimal)>> {
    let recs = sqlx::query!(
        "SELECT recorded_at, kwh FROM cumlative_amount_epower ORDER BY recorded_at DESC LIMIT $1",
        100
    )
    .fetch_all(pool)
    .await?;

    Ok(recs.iter().map(|a| (a.recorded_at, a.kwh)).collect())
}
