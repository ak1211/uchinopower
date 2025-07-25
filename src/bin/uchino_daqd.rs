// スマートメーターからデーターを収集してデーターベースに蓄積する。
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use anyhow::{Context, anyhow, bail};
use chrono::{DateTime, Datelike, Days, TimeDelta, TimeZone, Timelike, Utc};
use chrono_tz::Asia;
use cron::Schedule;
use daemonize::Daemonize;
use rust_decimal::Decimal;
use serialport::{DataBits, SerialPort, StopBits};
use sqlx::{self, QueryBuilder, postgres::PgPool};
use std::env;
use std::io::{self, BufReader};
use std::net::Ipv6Addr;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::LazyLock;
use std::time::Duration;
use tokio;
use tracing_appender;
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

/// 今日の積算電力量履歴を取得するechonet lite電文
static TODAY_CWH: LazyLock<EchonetliteFrame> = LazyLock::new(|| {
    EchonetliteFrame {
        ehd: 0x1081,              // 0x1081 = echonet lite
        tid: 1,                   // tid
        seoj: [0x05, 0xff, 0x01], // home controller
        deoj: [0x02, 0x88, 0x01], // smartmeter
        esv: 0x62,                // get要求
        opc: 1,                   // 1つ
        edata: vec![EchonetliteEdata {
            epc: 0xe2, // 積算電力量計測値履歴1
            pdc: 0,    // 今日
            edt: &[],
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
async fn exec_data_acquisition(port_name: &str, database_url: &str) -> anyhow::Result<()> {
    let pool = PgPool::connect(database_url).await?;

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

    loop {
        // スマートメーターと接続する
        authn::connect(
            &mut serial_port_reader,
            &mut serial_port,
            &credentials,
            &sender,
            settings.Channel,
            settings.PanId,
        )?;
        tokio::select! {
            // イベント受信用スレッド
            rx_result = smartmeter_receiver(&pool, &settings.Unit, &mut serial_port_reader) => {
                // スレッドは無限ループなのでここでは必ずエラー
                tracing::error!("rx_result:{:?}", rx_result);
            },
            // イベント送信用スレッド
            tx_result = smartmeter_transmitter(&sender, &mut serial_port) => {
                // スレッドは無限ループなのでここでは必ずエラー
                tracing::error!("tx_result:{:?}", tx_result);
            }
        }
        tracing::trace!("sleep");
        tokio::time::sleep(Duration::from_secs(30)).await;
        tracing::trace!("reconnect");
    }
}

/// 受信値をデーターベースに蓄積する
async fn commit_to_database<'a>(
    pool: &PgPool,
    unit: &SM::UnitForCumlativeAmountsPower,
    recorded_at: DateTime<Utc>,
    frame: &EchonetliteFrame<'a>,
) -> anyhow::Result<()> {
    match frame.esv {
        // Get_res プロパティ値読み出し応答
        0x72 => {
            for v in frame.edata.iter() {
                match SM::Properties::try_from(v.clone()) {
                    // 0xe2 積算電力量計測値履歴1 (正方向計測値)
                    Ok(SM::Properties::HistoricalCumlativeAmount(hist)) => {
                        let _ = commit_historical_cumlative_amount(&pool, unit, &hist).await?;
                    }
                    // 0xe7 瞬時電力計測値
                    Ok(SM::Properties::InstantiousPower(epower)) => {
                        let _ = commit_instant_epower(&pool, &recorded_at, &epower).await?;
                    }
                    // 0xe8 瞬時電流計測値
                    Ok(SM::Properties::InstantiousCurrent(current)) => {
                        let _ = commit_instant_current(&pool, &recorded_at, &current).await?;
                    }
                    // 0xea 定時積算電力量計測値(正方向計測値)
                    Ok(SM::Properties::CumlativeAmountsOfPowerAtFixedTime(epower)) => {
                        let _ = commit_cumlative_amount_epower(&pool, unit, &epower).await?;
                    }
                    //
                    _ => {}
                }
            }
        }
        // INF プロパティ値通知
        0x73 => {
            for v in frame.edata.iter() {
                match SM::Properties::try_from(v.clone()) {
                    // 0xea 定時積算電力量計測値(正方向計測値)
                    Ok(SM::Properties::CumlativeAmountsOfPowerAtFixedTime(epower)) => {
                        let _ = commit_cumlative_amount_epower(&pool, unit, &epower).await?;
                    }
                    //
                    _ => {}
                }
            }
        }
        //
        _esv => {}
    }
    Ok(())
}

/// ERXUDPイベント受信
async fn rx_erxudp(
    pool: &PgPool,
    unit: &SM::UnitForCumlativeAmountsPower,
    erxudp: Erxudp,
) -> anyhow::Result<()> {
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
    // 受信値をデーターベースに蓄積する
    commit_to_database(pool, unit, recorded_at, &frame).await?;
    // 受信値をログに出す
    let mut s = Vec::<String>::new();
    s.push(frame.show());
    for v in frame.edata.iter() {
        s.push(v.show(Some(unit)));
    }
    tracing::info!("{}", s.join(" "));
    Ok(())
}

#[tracing::instrument(skip_all)]
/// 受信
async fn smartmeter_receiver(
    pool: &PgPool,
    unit: &SM::UnitForCumlativeAmountsPower,
    serial_port_reader: &mut io::BufReader<dyn io::Read + Send>,
) -> anyhow::Result<()> {
    loop {
        match skstack::receive(serial_port_reader) {
            Ok(skstack::SkRxD::Void) => {}
            Ok(r @ skstack::SkRxD::Ok) => {
                tracing::trace!("{:?}", r);
            }
            Ok(r @ skstack::SkRxD::Fail(_)) => {
                tracing::trace!("{:?}", r);
            }
            Ok(skstack::SkRxD::Event(ev)) => match ev.code {
                0x01 => tracing::trace!("NS を受信した"),
                0x02 => tracing::trace!("NA を受信した"),
                0x05 => tracing::trace!("Echo Request を受信した"),
                0x1f => tracing::trace!("ED スキャンが完了した"),
                0x20 => tracing::trace!("Beacon を受信した"),
                0x21 if Some(0) == ev.param => tracing::trace!("UDP の送信に成功"),
                0x21 if Some(1) == ev.param => tracing::trace!("UDP の送信に失敗"),
                0x22 => tracing::trace!("アクティブスキャンが完了した"),
                0x24 => {
                    tracing::trace!("PANA による接続過程でエラーが発生した（接続が完了しなかった）")
                }
                0x25 => tracing::trace!("PANA による接続が完了した"),
                0x26 => tracing::trace!("接続相手からセッション終了要求を受信した"),
                0x27 => tracing::trace!("PANA セッションの終了に成功した"),
                0x28 => tracing::trace!(
                    "PANA セッションの終了要求に対する応答がなくタイムアウトした（セ
ッションは終了）"
                ),
                0x29 => bail!("セッションのライフタイムが経過して期限切れになった"),
                0x32 => tracing::trace!("ARIB108 の送信総和時間の制限が発動した"),
                0x33 => tracing::trace!("送信総和時間の制限が解除された"),
                _ => tracing::trace!("{:?}", ev),
            },
            Ok(r @ skstack::SkRxD::Epandesc(_)) => {
                tracing::trace!("{:?}", r);
            }
            Ok(skstack::SkRxD::Erxudp(erxudp)) => {
                rx_erxudp(pool, unit, erxudp).await?;
            }
            Err(e) if e.kind() == io::ErrorKind::TimedOut => {} // タイムアウトエラーは無視する
            Err(e) => return Err(e).context("serial port read failed!"),
        }
        // 制御を他のタスクに譲る
        tokio::task::yield_now().await;
    }
}

#[tracing::instrument(skip_all)]
/// 送信
async fn smartmeter_transmitter<T: io::Write + Send>(
    sender: &Ipv6Addr,
    serial_port: &mut T,
) -> anyhow::Result<()> {
    // メッセージ送信(今日の積算電力量履歴)
    skstack::send_echonetlite(serial_port, &sender, &TODAY_CWH)?;

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

/// 今日の積算電力量履歴をデーターベースに蓄積する
async fn commit_historical_cumlative_amount(
    pool: &PgPool,
    unit: &SM::UnitForCumlativeAmountsPower,
    hist: &SM::HistoricalCumlativeAmount,
) -> anyhow::Result<()> {
    let jst_now = Utc::now().with_timezone(&Asia::Tokyo);
    let jst_today = Asia::Tokyo
        .with_ymd_and_hms(jst_now.year(), jst_now.month(), jst_now.day(), 0, 0, 0)
        .single()
        .unwrap();
    let day = jst_today
        .checked_sub_days(Days::new(hist.n_days_ago as u64))
        .with_context(|| format!("n_days_ago:{}", hist.n_days_ago))?;
    let halfhour = TimeDelta::new(30 * 60, 0).unwrap();
    //
    let mut accumulator = day;
    let timeserial = std::iter::from_fn(move || {
        let ret = Some(accumulator);
        accumulator = accumulator.checked_add_signed(halfhour).unwrap();
        ret
    });
    //
    let histrical_kwh = hist
        .historical
        .iter()
        .zip(timeserial)
        .map(|(opt_val, datetime)| -> Option<(DateTime<Utc>, Decimal)> {
            match opt_val {
                Some(val) => {
                    let kwh = Decimal::from(*val) * unit.0;
                    Some((datetime.with_timezone(&Utc), kwh))
                }
                None => None,
            }
        })
        .flatten()
        .collect::<Vec<(DateTime<Utc>, Decimal)>>();
    //
    let mut query_builder =
        QueryBuilder::new("INSERT INTO cumlative_amount_epower ( recorded_at, kwh ) ");

    query_builder.push_values(histrical_kwh, |mut b, value| {
        b.push_bind(value.0).push_bind(value.1);
    });

    let query = query_builder.build();
    query.execute(pool).await?;

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let file_appender = tracing_appender::rolling::daily("/var/log", "uchino_daqd.log");
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
        .with_file(false)
        .with_line_number(false)
        .with_thread_names(true)
        .with_thread_ids(true)
        .with_ansi(false)
        .with_writer(file_appender)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let launcher = async || -> anyhow::Result<()> {
        // 環境変数
        let serial_device = env::var("SERIAL_DEVICE").context("Must be set to SERIAL_DEVICE")?;
        let database_url = env::var("DATABASE_URL").context("Must be set to DATABASE_URL")?;

        let daemonize = Daemonize::new()
            .pid_file("/run/uchino_daqd.pid")
            .working_directory("/tmp")
            .user("nobody")
            .group("dialout");
        daemonize.start()?;
        exec_data_acquisition(&serial_device, &database_url).await
    };

    match launcher().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{}", e);
            ExitCode::FAILURE
        }
    }
}
