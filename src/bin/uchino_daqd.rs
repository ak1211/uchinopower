// スマートメーターからデーターを収集してデーターベースに蓄積する。
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use anyhow::{Context, anyhow};
use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use chrono_tz::Asia;
use cron::Schedule;
use daemonize::Daemonize;
use rust_decimal::Decimal;
use serialport::{DataBits, SerialPort, StopBits};
use sqlx::{self, postgres::PgPool};
use std::env;
use std::fs::OpenOptions;
use std::io::{self, BufReader};
use std::net::Ipv6Addr;
use std::str::FromStr;
use std::sync::LazyLock;
use std::time::Duration;
use tokio;
use tracing_subscriber::FmtSubscriber;
use uchinoepower::connection_settings::ConnectionSettings;
use uchinoepower::echonetlite::{
    EchonetliteEdata, EchonetliteFrame, smart_electric_energy_meter as SM,
};
use uchinoepower::skstack::{self, Erxudp, authn};

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

/// 定時積算電力量計測値を取得するechonet lite電文
static LATEST_CWH: LazyLock<EchonetliteFrame> = LazyLock::new(|| {
    EchonetliteFrame {
        ehd: 0x1081,              // 0x1081 = echonet lite
        tid: 1,                   // tid
        seoj: [0x05, 0xff, 0x01], // home controller
        deoj: [0x02, 0x88, 0x01], // smartmeter
        esv: 0x62,                // get要求
        opc: 1,                   // 1つ
        edata: vec![EchonetliteEdata {
            epc: 0xea, // 定時積算電力量計測値(正方向計測値)
            ..Default::default()
        }],
    }
});

/// 瞬時電力と瞬時電流計測値を取得するechonet lite電文
static INSTANT_WATT_AMPERE: LazyLock<EchonetliteFrame> = LazyLock::new(|| {
    EchonetliteFrame {
        ehd: 0x1081,              // 0x1081 = echonet lite
        tid: 1,                   // tid
        seoj: [0x05, 0xff, 0x01], // home controller
        deoj: [0x02, 0x88, 0x01], // smartmeter
        esv: 0x62,                // get要求
        opc: 2,                   // 2つ
        edata: vec![
            EchonetliteEdata {
                epc: 0xe7, // 瞬時電力計測値
                ..Default::default()
            },
            EchonetliteEdata {
                epc: 0xe8, // 瞬時電流計測値
                ..Default::default()
            },
        ],
    }
});

/// スマートメーターからデーターを収集する
async fn exec_data_acquisition(port_name: &str, pool: &PgPool) -> anyhow::Result<()> {
    // データベースからスマートメーターの情報を得る
    let settings = read_settings(&pool).await?;
    let credentials = authn::Credentials {
        id: authn::Id::from_str(&settings.RouteBId).map_err(|s| anyhow!(s))?,
        password: authn::Password::from_str(&settings.RouteBPassword).map_err(|s| anyhow!(s))?,
    };
    let mac_address =
        u64::from_str_radix(&settings.MacAddress, 16).context("MacAddress parse error")?;

    // MACアドレスからIPv6リンクローカルアドレスへ変換する
    // MACアドレスの最初の1バイト下位2bit目を反転して
    // 0xFE80000000000000XXXXXXXXXXXXXXXXのXXをMACアドレスに置き換える
    let sender = Ipv6Addr::from_bits(
        0xFE80_0000_0000_0000u128 << 64 | (mac_address as u128 ^ 0x0200_0000_0000_0000u128),
    );

    // シリアルポートを開く
    let mut serial_port = open_port(port_name)?;

    // シリアルポート読み込みはバッファリングする
    let mut serial_port_reader = serial_port
        .try_clone()
        .and_then(|cloned| Ok(BufReader::new(cloned)))
        .context("Failed to clone")?;

    // スマートメーターと接続する
    authn::connect(
        &mut serial_port_reader,
        &mut serial_port,
        &credentials,
        &sender,
        settings.Channel,
        settings.PanId,
    )?;

    loop {
        tokio::select! {
            // イベント受信用スレッド
            rx_result = smartmeter_receiver(&pool, &mut serial_port_reader, &settings.Unit) => {
                // スレッドは無限ループなのでここでは必ずエラー
                tracing::error!("rx_result:{:?}", rx_result);
            },
            // イベント送信用スレッド
            tx_result = smartmeter_transmitter(&mut serial_port, &sender) => {
                // スレッドは無限ループなのでここでは必ずエラー
                tracing::error!("tx_result:{:?}", tx_result);
            }
        }
    }
}

/// ERXUDPイベント受信
fn rx_erxudp(serial_port_reader: &mut BufReader<dyn io::Read>) -> anyhow::Result<Option<Erxudp>> {
    match skstack::receive(serial_port_reader) {
        Ok(r @ skstack::SkRxD::Ok) => {
            tracing::trace!("{:?}", r);
        }
        Ok(r @ skstack::SkRxD::Fail(_)) => {
            tracing::trace!("{:?}", r);
        }
        Ok(r @ skstack::SkRxD::Event(_)) => {
            tracing::trace!("{:?}", r);
        }
        Ok(r @ skstack::SkRxD::Epandesc(_)) => {
            tracing::trace!("{:?}", r);
        }
        Ok(skstack::SkRxD::Erxudp(v)) => {
            return Ok(Some(v));
        }
        Err(e) if e.kind() == io::ErrorKind::TimedOut => {} // タイムアウトエラーは無視する
        Err(e) => return Err(e).context("serial port read failed!"),
    }
    Ok(None)
}

/// 受信
async fn smartmeter_receiver(
    pool: &PgPool,
    serial_port_reader: &mut io::BufReader<dyn io::Read + Send>,
    unit: &SM::UnitForCumlativeAmountsPower,
) -> anyhow::Result<()> {
    loop {
        if let Some(erxudp) = rx_erxudp(serial_port_reader)? {
            // 受信時刻(分単位)
            let recorded_at = {
                let jst = Utc::now().with_timezone(&Asia::Tokyo);
                let modified = Asia::Tokyo
                    .with_ymd_and_hms(
                        jst.year(),
                        jst.month(),
                        jst.day(),
                        jst.hour(),
                        jst.minute(),
                        0,
                    )
                    .single()
                    .unwrap();
                modified.with_timezone(&Utc)
            };
            // ERXUDPメッセージからEchonetliteフレームを取り出す。
            let config = bincode::config::standard()
                .with_big_endian()
                .with_fixed_int_encoding();
            let (frame, _len): (EchonetliteFrame, usize) =
                bincode::borrow_decode_from_slice(&erxudp.data, config).unwrap();
            match frame.esv {
                // Get_res プロパティ値読み出し応答
                0x72 => {
                    for v in frame.edata.iter() {
                        match SM::Properties::try_from(v.clone()) {
                            // 0xe7 瞬時電力計測値
                            Ok(SM::Properties::InstantiousPower(epower)) => {
                                if let Err(e) =
                                    commit_instant_epower(&pool, &recorded_at, &epower).await
                                {
                                    tracing::error!("{}", e);
                                }
                            }
                            // 0xe8 瞬時電流計測値
                            Ok(SM::Properties::InstantiousCurrent(current)) => {
                                if let Err(e) =
                                    commit_instant_current(&pool, &recorded_at, &current).await
                                {
                                    tracing::error!("{}", e);
                                }
                            }
                            // 0xea 定時積算電力量計測値(正方向計測値)
                            Ok(SM::Properties::CumlativeAmountsOfPowerAtFixedTime(epower)) => {
                                if let Err(e) =
                                    commit_cumlative_amount_epower(&pool, unit, &epower).await
                                {
                                    tracing::error!("{}", e);
                                }
                            }
                            //
                            _ => {
                                tracing::trace!(
                                    "Unhandled ESV {}, EPC {} {:?}",
                                    frame.esv,
                                    v.epc,
                                    v.show(Some(unit))
                                );
                            }
                        }
                    }
                }
                // INF プロパティ値通知
                0x73 => {
                    for v in frame.edata.iter() {
                        match SM::Properties::try_from(v.clone()) {
                            // 0xea 定時積算電力量計測値(正方向計測値)
                            Ok(SM::Properties::CumlativeAmountsOfPowerAtFixedTime(epower)) => {
                                if let Err(e) =
                                    commit_cumlative_amount_epower(&pool, unit, &epower).await
                                {
                                    tracing::error!("{}", e);
                                }
                            }
                            //
                            _ => {
                                tracing::trace!(
                                    "Unhandled ESV {}, EPC {} {:?}",
                                    frame.esv,
                                    v.epc,
                                    v.show(Some(unit))
                                );
                            }
                        }
                    }
                }
                //
                esv => tracing::trace!("Unhandled ESV:{esv}"),
            }
            let mut s = Vec::<String>::new();
            s.push(frame.show());
            for v in frame.edata.iter() {
                s.push(v.show(Some(unit)));
            }
            tracing::info!("{}", s.join(" "));
        }
        // 制御を他のタスクに譲る
        tokio::task::yield_now().await;
    }
}

/// 送信
async fn smartmeter_transmitter<T: io::Write + Send>(
    serial_port: &mut T,
    sender: &Ipv6Addr,
) -> anyhow::Result<()> {
    // メッセージ送信(定時積算電力量計測値)
    skstack::send_echonetlite(serial_port, &sender, &LATEST_CWH)?;

    // スケジュールに則りメッセージ送信
    let schedule = Schedule::from_str("00 */1 * * * *")?;
    for next in schedule.upcoming(Asia::Tokyo) {
        // 次回実行予定時刻まで待つ
        let duration = (next.to_utc() - Utc::now()).to_std()?;
        tracing::trace!("Next scheduled time. ({}), sleep ({:?})", next, duration);
        tokio::time::sleep(duration).await;
        // メッセージ送信(瞬時電力と瞬時電流計測値)
        skstack::send_echonetlite(serial_port, &sender, &INSTANT_WATT_AMPERE)?;
    }
    Ok(())
}

/// 設定情報をデーターベースから得る
async fn read_settings(pool: &PgPool) -> anyhow::Result<ConnectionSettings> {
    #[derive(sqlx::FromRow)]
    #[allow(dead_code)]
    struct Row {
        id: i64,
        note: sqlx::types::Json<ConnectionSettings>,
    }

    let row = sqlx::query_as!(
        Row,
        r#"
SELECT id, note as "note: sqlx::types::Json<ConnectionSettings>"
FROM settings
ORDER BY id DESC
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(row.note.0)
}

/// 瞬時電力をデーターベースに蓄積する
async fn commit_instant_epower(
    pool: &PgPool,
    recorded_at: &DateTime<Utc>,
    epower: &SM::InstantiousPower,
) -> anyhow::Result<i64> {
    let rec = sqlx::query!(
        r#"
INSERT INTO instant_epower ( recorded_at, watt )
VALUES ( $1, $2 )
RETURNING id
        "#,
        *recorded_at,
        epower.0
    )
    .fetch_one(pool)
    .await?;

    Ok(rec.id)
}

/// 瞬時電流をデーターベースに蓄積する
async fn commit_instant_current(
    pool: &PgPool,
    recorded_at: &DateTime<Utc>,
    current: &SM::InstantiousCurrent,
) -> anyhow::Result<i64> {
    let rec = sqlx::query!(
        r#"
INSERT INTO instant_current ( recorded_at, r, t )
VALUES ( $1, $2, $3 )
RETURNING id
        "#,
        *recorded_at,
        current.r,
        current.t
    )
    .fetch_one(pool)
    .await?;

    Ok(rec.id)
}

/// 定時積算電力量計測値(正方向計測値)をデーターベースに蓄積する
async fn commit_cumlative_amount_epower(
    pool: &PgPool,
    unit: &SM::UnitForCumlativeAmountsPower,
    epower: &SM::CumlativeAmountsOfPowerAtFixedTime,
) -> anyhow::Result<i64> {
    let jst = Asia::Tokyo
        .with_ymd_and_hms(
            epower.time_point.year(),
            epower.time_point.month(),
            epower.time_point.day(),
            epower.time_point.hour(),
            epower.time_point.minute(),
            epower.time_point.second(),
        )
        .single()
        .unwrap();
    let kwh = Decimal::from(epower.cumlative_amounts_power) * unit.0;
    let rec = sqlx::query!(
        r#"
INSERT INTO cumlative_amount_epower ( recorded_at, kwh )
VALUES ( $1, $2 )
RETURNING id
        "#,
        jst.with_timezone(&Utc),
        kwh
    )
    .fetch_one(pool)
    .await?;

    Ok(rec.id)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .with_thread_names(true)
        .with_thread_ids(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // 環境変数
    let serial_device = env::var("SERIAL_DEVICE").context("Must be set to SERIAL_DEVICE")?;
    let database_url = env::var("DATABASE_URL").context("Must be set to DATABASE_URL")?;

    let stdout = OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open("/run/uchino_daqd.out")
        .context("stdout file create error")?;

    let stderr = OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open("/run/uchino_daqd.err")
        .context("stderr file create error")?;

    let daemonize = Daemonize::new()
        .pid_file("/run/uchino_daqd.pid")
        .working_directory("/tmp")
        .user("nobody")
        .group("dialout")
        .stdout(stdout)
        .stderr(stderr);

    match daemonize.start() {
        Ok(_) => {
            let pool = PgPool::connect(&database_url).await?;
            if let Err(e) = exec_data_acquisition(&serial_device, &pool).await {
                tracing::error!("{}", e);
            }
        }
        Err(e) => tracing::error!("{}", e),
    }
    Ok(())
}
