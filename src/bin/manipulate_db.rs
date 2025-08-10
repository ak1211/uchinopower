// 測定値データーベースをいじる
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use anyhow::Context;
use chrono::{DateTime, Utc};
use chrono_tz::Asia;
use clap::{Args, Parser, Subcommand};
use futures_util::TryStreamExt;
use rust_decimal::Decimal;
use sqlx::{self, postgres::PgPool};
use std::result;

/// 測定値データーベースをいじる
#[derive(Parser, Debug)]
#[command(name = "manipulate_db")]
#[command(version, about, long_about = None)]
struct Cli {
    /// データベースURL
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// 測定値を得る
    #[clap(alias = "get-record")]
    Get(GetArgs),
    /// 測定値の重複を整理する
    #[clap(alias = "unique-record")]
    Unique(UniqueArgs),
}

#[derive(Debug, Args)]
struct GetArgs {
    /// レコード数
    #[arg(short = 'C', long, default_value_t = 10)]
    count: u32,
}

#[derive(Debug, Args)]
struct UniqueArgs {
    #[arg(long, action)]
    dryrun: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    // コマンドライン引数
    let cli = Cli::parse();

    let pool = PgPool::connect(&cli.database_url)
        .await
        .context("データベースとの接続失敗")?;

    match &cli.command {
        Commands::Get(args) => exec_get_record(&pool, args).await,
        Commands::Unique(args) => exec_unique_record(&pool, args).await,
    }
}

/// 測定値の重複を整理する
async fn exec_unique_record(pool: &PgPool, args: &UniqueArgs) -> anyhow::Result<()> {
    #[derive(sqlx::FromRow, Eq, PartialEq, Default)]
    struct Measure {
        id: i64,
        recorded_at: DateTime<Utc>,
        kwh: Decimal,
    }

    let mut delete_id = Vec::<i64>::new();

    let mut rows = sqlx::query_as::<_, Measure>(
        "SELECT id, recorded_at, kwh FROM cumlative_amount_epower ORDER BY recorded_at",
    )
    .fetch(pool);

    let mut unique_record: Measure = Default::default();
    while let Some(row) = rows.try_next().await? {
        print!("{}, {}, {}", row.id, row.recorded_at, row.kwh);
        if unique_record.recorded_at == row.recorded_at && unique_record.kwh == row.kwh {
            print!(" **This record is same as id {}**", unique_record.id);
            delete_id.push(row.id);
        }
        println!("");
        unique_record = row;
    }

    if !args.dryrun {
        //
        let transaction = pool.begin().await.context("transaction error")?;
        //
        let mut counter = 0;
        for id in delete_id {
            sqlx::query("DELETE FROM cumlative_amount_epower WHERE id = $1")
                .bind(id)
                .execute(pool)
                .await?;
            println!("id: {} has been deleted.", id);
            counter = counter + 1;
        }
        println!("Total {} records deleted.", counter);
        //
        transaction.commit().await.context("commit failure")?;
    }

    Ok(())
}

/// 測定値を得る
async fn exec_get_record(pool: &PgPool, args: &GetArgs) -> anyhow::Result<()> {
    //
    let xs = read_instant_epower(&pool, args.count as i64).await?;
    println!("time, instantious electric power(W)");
    for (at, power) in xs.iter() {
        let t = at.with_timezone(&Asia::Tokyo).to_rfc3339();
        println!("{t}, {power}");
    }
    println!("");

    let xs = read_instant_current(&pool, args.count as i64).await?;
    println!("time, instantious current R(A), T(A)");
    for (at, ir, it) in xs.iter() {
        let t = at.with_timezone(&Asia::Tokyo).to_rfc3339();
        println!(
            "{t}, {ir}{}",
            it.map(|v| format!(", {v}")).unwrap_or_default()
        );
    }
    println!("");

    let xs = read_cumlative_amount_epower(&pool, args.count as i64).await?;
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
    count: i64,
) -> result::Result<Vec<(DateTime<Utc>, Decimal)>, sqlx::Error> {
    let mut recs = sqlx::query!(
        "SELECT recorded_at, watt FROM instant_epower ORDER BY recorded_at DESC LIMIT $1",
        count
    )
    .fetch_all(pool)
    .await?;

    recs.reverse();
    Ok(recs.iter().map(|a| (a.recorded_at, a.watt)).collect())
}

/// 瞬時電流をデーターベースから得る
async fn read_instant_current(
    pool: &PgPool,
    count: i64,
) -> result::Result<Vec<(DateTime<Utc>, Decimal, Option<Decimal>)>, sqlx::Error> {
    let mut recs = sqlx::query!(
        "SELECT recorded_at, r, t FROM instant_current ORDER BY recorded_at DESC LIMIT $1",
        count
    )
    .fetch_all(pool)
    .await?;

    recs.reverse();
    Ok(recs.iter().map(|a| (a.recorded_at, a.r, a.t)).collect())
}

/// 定時積算電力量計測値(正方向計測値)をデーターベースから得る
async fn read_cumlative_amount_epower(
    pool: &PgPool,
    count: i64,
) -> result::Result<Vec<(DateTime<Utc>, Decimal)>, sqlx::Error> {
    let mut recs = sqlx::query!(
        "SELECT recorded_at, kwh FROM cumlative_amount_epower ORDER BY recorded_at DESC LIMIT $1",
        count
    )
    .fetch_all(pool)
    .await?;

    recs.reverse();
    Ok(recs.iter().map(|a| (a.recorded_at, a.kwh)).collect())
}
