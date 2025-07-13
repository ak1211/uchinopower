// アクティブスキャンでスマートメーターを探して接続情報をデーターベースに蓄積する。
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use anyhow::{Context, anyhow};
use clap::Parser;
use env_logger;
use serialport::{DataBits, SerialPort, StopBits};
use sqlx::PgPool;
use std::env;
use std::io::BufReader;
use std::str::FromStr;
use std::time::Duration;
use uchinoepower::pairing;
use uchinoepower::skstack::authn;

/// 接続対象のスマートメーターを探す
#[derive(Parser, Debug)]
#[command(name = "pairing")]
#[command(version, about, long_about = None)]
struct Cli {
    /// データベースURL
    #[arg(long)]
    database_url: Option<String>,

    /// シリアルデバイス名
    #[arg(short = 'D', long, default_value = "/dev/ttyUSB0")]
    device: String,

    /// アクティブスキャン時間(1～14)
    #[arg(short = 'T', long, default_value_t = 6)]
    activescan: usize,

    /// ルートBID(32文字)
    id: String,

    /// ルートBパスワード(12文字)
    password: String,
}

/// シリアルポートを開く
fn open_port(port_name: &str) -> anyhow::Result<Box<dyn SerialPort>> {
    let builder = serialport::new(port_name, 115200)
        .stop_bits(StopBits::One)
        .data_bits(DataBits::Eight)
        .timeout(Duration::from_secs(1));

    builder
        .open()
        .with_context(move || format!("Failed to open \"{}\".", port_name))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let _ = dotenv::dotenv();

    // デバッグレベルは RUST_LOG 環境変数で設定できる
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));

    // コマンドライン引数
    let cli = Cli::parse();

    let credentials = authn::Credentials {
        id: authn::Id::from_str(&cli.id).map_err(|s| anyhow!(s))?,
        password: authn::Password::from_str(&cli.password).map_err(|s| anyhow!(s))?,
    };

    if let Some(database_url) = cli
        .database_url
        .or(env::var("DATABASE_URL").map(|a| a.to_string()).ok())
    {
        // データーベースプール
        let pool = PgPool::connect(&database_url).await?;

        // シリアルポートを開く
        let mut port = open_port(&cli.device)?;

        // シリアルポート読み込みはバッファリングする
        let mut reader = port
            .try_clone()
            .and_then(|cloned| Ok(BufReader::new(cloned)))
            .context("Failed to clone")?;

        // 接続するスマートメーターをアクティブスキャンで探して設定ファイルに情報を保存する
        match pairing(&mut reader, &mut port, cli.activescan, &credentials)? {
            Some(settings) => {
                // データーベースに蓄積する
                let rec = sqlx::query!(
                    "INSERT INTO settings ( note ) VALUES ( $1 ) RETURNING id",
                    sqlx::types::Json(settings) as _
                )
                .fetch_one(&pool)
                .await?;
                Ok(println!("successfully finished, id={}", rec.id))
            }
            None => Ok(println!("Could not find smart meter.")),
        }
    } else {
        println!("DATABASE_URL を指定してください。");
        Ok(())
    }
}
