// スマートメーターからデーターを収集してデーターベースに蓄積する。
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use chrono::{DateTime, Datelike, Days, TimeDelta, TimeZone, Timelike, Utc};
use chrono_tz::Asia;
use cron::Schedule;
use rust_decimal::Decimal;
use serialport::{DataBits, StopBits};
use sqlx::{self, QueryBuilder, postgres::PgPool};
use std::env;
use std::io::{self, BufReader};
use std::net::Ipv6Addr;
use std::process::ExitCode;
use std::result;
use std::str::FromStr;
use std::sync::LazyLock;
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio;
use tracing::{Event, Subscriber};
use tracing_subscriber::{
    fmt::{self, FormatEvent, FormatFields},
    layer::SubscriberExt,
    registry::LookupSpan,
    util::SubscriberInitExt,
};
use uchinoepower::connection_settings::ConnectionSettings;
use uchinoepower::echonetlite::{
    EchonetliteEdata, EchonetliteFrame, smart_electric_energy_meter as SM,
};
use uchinoepower::skstack::{self, Erxudp, authn};

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[derive(Debug, Error)]
pub enum DaqDaemonError {
    #[error(r#"i/o "{0}""#)]
    Io(#[from] io::Error),

    #[error(r#"binary encode "{0}""#)]
    BinaryEncode(#[from] bincode::error::EncodeError),

    #[error(r#"cron "{0}""#)]
    Cron(#[from] cron::error::Error),

    #[error(r#"out of range "{0}""#)]
    OutOfRange(#[from] chrono::OutOfRangeError),

    #[error(r#"serial port "{0}""#)]
    SerialPort(#[from] serialport::Error),

    #[error(r#"database "{0}""#)]
    Database(#[from] sqlx::Error),

    #[error(r#"invalid id "{0}""#)]
    InvalidId(String),

    #[error(r#"invalid password "{0}""#)]
    InvalidPassword(String),

    #[error("invalid mac address")]
    InvalidMacAddress,

    #[error("fail. code: {0:X}(hex)")]
    CommandFail(u8),

    #[error("PANA session disconnected")]
    PanaSessionDisconnected,

    #[error("{0}")]
    Other(&'static str),
}

impl From<authn::Error> for DaqDaemonError {
    fn from(err: authn::Error) -> DaqDaemonError {
        match err {
            authn::Error::Fail(code) => DaqDaemonError::CommandFail(code),
            authn::Error::Io(e) => DaqDaemonError::Io(e),
            authn::Error::PanaSessionDisconnected => DaqDaemonError::PanaSessionDisconnected,
        }
    }
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

/// 受信値をデーターベースに蓄積する
async fn commit_to_database<'a>(
    pool: &PgPool,
    unit: &SM::UnitForCumlativeAmountsPower,
    recorded_at: &DateTime<Utc>,
    frame: &EchonetliteFrame<'a>,
) -> result::Result<(), DaqDaemonError> {
    for edata in frame.edata.iter() {
        match SM::Properties::try_from(edata) {
            // 0xe2 積算電力量計測値履歴1 (正方向計測値)
            Ok(SM::Properties::HistoricalCumlativeAmount(hist)) => {
                commit_historical_cumlative_amount(&pool, unit, &hist).await?;
            }
            // 0xe7 瞬時電力計測値
            Ok(SM::Properties::InstantiousPower(epower)) => {
                commit_instant_epower(&pool, recorded_at, &epower).await?;
            }
            // 0xe8 瞬時電流計測値
            Ok(SM::Properties::InstantiousCurrent(current)) => {
                commit_instant_current(&pool, recorded_at, &current).await?;
            }
            // 0xea 定時積算電力量計測値(正方向計測値)
            Ok(SM::Properties::CumlativeAmountsOfPowerAtFixedTime(epower)) => {
                commit_cumlative_amount_epower(&pool, unit, &epower).await?;
            }
            //
            Ok(v) => tracing::warn!(r#"This data "{v}" is not committed to the database"#),
            Err(e) => tracing::error!("{e}"),
        }
    }
    Ok(())
}

/// ERXUDPイベント受信
async fn rx_erxudp(
    pool: &PgPool,
    unit: &SM::UnitForCumlativeAmountsPower,
    erxudp: &Erxudp,
) -> result::Result<(), DaqDaemonError> {
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
            .ok_or(DaqDaemonError::Other("time calculate error"))?;
        modified.with_timezone(&Utc)
    };

    let dump = |xs: &Vec<u8>| xs.iter().map(|b| format!("{:02X}", b)).collect::<String>();

    match erxudp.destination_port {
        // UDPポート番号 0E1A = 3610 は Echonetliteメッセージ
        0x0e1a => {
            // ERXUDPメッセージからEchonetliteフレームを取り出す。
            let config = bincode::config::standard()
                .with_big_endian()
                .with_fixed_int_encoding();

            let decoded: Result<(EchonetliteFrame, usize), _> =
                bincode::borrow_decode_from_slice(&erxudp.data, config);

            match decoded {
                Ok((frame, _len)) => {
                    // 受信値をデーターベースに蓄積する
                    commit_to_database(pool, unit, &recorded_at, &frame).await?;
                    // 受信値をログに出す
                    let mut s = Vec::<String>::new();
                    s.push(frame.show());
                    for v in frame.edata.iter() {
                        s.push(v.show(Some(unit)));
                    }
                    tracing::info!("{}", s.join(" "));
                }
                Err(e) => {
                    tracing::error!(
                        r#"Echonetlite message "{}" parse error, reason:{}"#,
                        dump(&erxudp.data),
                        e
                    );
                }
            }
        }
        // UDPポート番号 02CC = 716 は PANAメッセージ(RFC5191)
        0x02cc => {
            tracing::warn!(r#"PANA message "{}" is IGNORED"#, dump(&erxudp.data));
            return Ok(());
        }
        // 未知のUDPポート番号
        rport => {
            tracing::warn!(
                r#"rport {rport} message "{}" is UNKNOWN and IGNORED."#,
                dump(&erxudp.data)
            );
        }
    }
    Ok(())
}

/// 設定情報をデーターベースから得る
async fn read_settings(pool: &PgPool) -> result::Result<ConnectionSettings, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    #[allow(dead_code)]
    struct Row {
        id: i64,
        note: sqlx::types::Json<ConnectionSettings>,
    }

    let row = sqlx::query_as!(
        Row,
        r#"SELECT id, note as "note: sqlx::types::Json<ConnectionSettings>" FROM settings ORDER BY id DESC"#
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
) -> result::Result<i64, DaqDaemonError> {
    let rec = sqlx::query!(
        r#"INSERT INTO instant_epower ( recorded_at, watt ) VALUES ( $1, $2 ) RETURNING id"#,
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
) -> result::Result<i64, DaqDaemonError> {
    let rec = sqlx::query!(
        r#"INSERT INTO instant_current ( recorded_at, r, t ) VALUES ( $1, $2, $3 ) RETURNING id"#,
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
) -> result::Result<i64, DaqDaemonError> {
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
        .ok_or(DaqDaemonError::Other("time calculate error"))?;
    let kwh = Decimal::from(epower.cumlative_amounts_power) * unit.0;
    let rec = sqlx::query!(
        r#"INSERT INTO cumlative_amount_epower ( recorded_at, kwh ) VALUES ( $1, $2 ) RETURNING id"#,
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
) -> result::Result<(), DaqDaemonError> {
    // 現在時刻
    let jst_now = Utc::now().with_timezone(&Asia::Tokyo);

    // 現在時刻 - hist.n_days_ago 日の午前０時ちょうど
    let day = Asia::Tokyo
        .with_ymd_and_hms(jst_now.year(), jst_now.month(), jst_now.day(), 0, 0, 0)
        .single()
        .and_then(|jst_today| jst_today.checked_sub_days(Days::new(hist.n_days_ago as u64)))
        .ok_or(DaqDaemonError::Other("time calculate error"))?;

    // 30分間隔のTimeDelta
    let halfhour =
        TimeDelta::new(30 * 60, 0).ok_or(DaqDaemonError::Other("time calculate error"))?;

    // 本日の午前０時ちょうどから30分毎の時刻列を作成するイテレータ
    let mut accumulator = Some(day);
    let timeserial = std::iter::from_fn(move || {
        let ret = accumulator;
        accumulator = accumulator.and_then(|v| v.checked_add_signed(halfhour));
        ret
    });

    // 時間と積算電力量の組を作成する
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

    let mut query_builder =
        QueryBuilder::new(r#"INSERT INTO cumlative_amount_epower (recorded_at, kwh)"#);

    query_builder.push_values(histrical_kwh, |mut b, value| {
        b.push_bind(value.0).push_bind(value.1);
    });

    let query = query_builder.build();
    query.execute(pool).await?;

    Ok(())
}

#[tracing::instrument(skip_all)]
/// 送信
async fn smartmeter_transmitter<T: io::Write + Send>(
    sender: &Ipv6Addr,
    session_rejoin_period: Duration,
    serial_port: &mut T,
) -> result::Result<(), DaqDaemonError> {
    // メッセージ送信(今日の積算電力量履歴)
    let command = skstack::command_from_echonetliteframe(&sender, &TODAY_CWH)?;
    skstack::send(serial_port, &command)?;

    let mut rejoin_time = Instant::now() + session_rejoin_period;

    // スケジュールに則りメッセージ送信
    let schedule = Schedule::from_str("00 */1 * * * *")?;
    for next in schedule.upcoming(Asia::Tokyo) {
        // 次回実行予定時刻まで待つ
        let duration = (next.to_utc() - Utc::now()).to_std()?;
        tracing::trace!("Next scheduled time. ({}), sleep ({:?})", next, duration);
        tokio::time::sleep(duration).await;
        // メッセージ送信(瞬時電力と瞬時電流計測値)
        let command = skstack::command_from_echonetliteframe(&sender, &INSTANT_WATT_AMPERE)?;
        skstack::send(serial_port, &command)?;
        // 再認証を要求する
        let now = Instant::now();
        if now >= rejoin_time {
            tokio::time::sleep(Duration::from_secs(1)).await;
            skstack::send(serial_port, b"SKREJOIN\r\n")?;
            rejoin_time = now + session_rejoin_period;
        }
    }
    Ok(())
}

#[tracing::instrument(skip_all)]
/// 受信
async fn smartmeter_receiver<T: io::Read + Send + 'static>(
    pool: &PgPool,
    settings: &ConnectionSettings,
    serial_port_reader: &mut BufReader<T>,
) -> result::Result<(), DaqDaemonError> {
    loop {
        match skstack::receive(serial_port_reader) {
            Ok(skstack::SkRxD::Void) => {}
            Ok(r @ skstack::SkRxD::Ok) => tracing::trace!("{r:?}"),
            Ok(skstack::SkRxD::Fail(code)) => {
                tracing::error!("コマンド実行に失敗した。{code:X}(hex)");
                return Err(DaqDaemonError::CommandFail(code));
            }
            Ok(skstack::SkRxD::Event(event)) => match event.code {
                0x01 => tracing::trace!("NS を受信した"),
                0x02 => tracing::trace!("NA を受信した"),
                0x05 => tracing::trace!("Echo Request を受信した"),
                0x1f => tracing::trace!("ED スキャンが完了した"),
                0x20 => tracing::trace!("Beacon を受信した"),
                0x21 if Some(0) == event.param => tracing::trace!("UDP の送信に成功"),
                0x21 if Some(1) == event.param => tracing::trace!("UDP の送信に失敗"),
                0x21 if Some(2) == event.param => {
                    tracing::trace!("UDP を送信する代わりにアドレス要請を行った")
                }
                0x22 => tracing::trace!("アクティブスキャンが完了した"),
                0x24 => {
                    tracing::trace!(
                        "PANA による接続過程でエラーが発生した（接続が完了しなかった）"
                    );
                    return Err(DaqDaemonError::PanaSessionDisconnected);
                }
                0x25 => tracing::trace!("PANA による接続が完了した"),
                0x26 => tracing::trace!("接続相手からセッション終了要求を受信した"),
                0x27 => {
                    tracing::trace!("PANA セッションの終了に成功した");
                    return Err(DaqDaemonError::PanaSessionDisconnected);
                }
                0x28 => {
                    tracing::trace!(
                        "PANA セッションの終了要求に対する応答がなくタイムアウトした（セッションは終了）"
                    );
                    return Err(DaqDaemonError::PanaSessionDisconnected);
                }
                0x29 => {
                    tracing::trace!("セッションのライフタイムが経過して期限切れになった");
                    return Err(DaqDaemonError::PanaSessionDisconnected);
                }
                0x32 => tracing::trace!("ARIB108 の送信総和時間の制限が発動した"),
                0x33 => tracing::trace!("送信総和時間の制限が解除された"),
                _ => tracing::trace!("{event:?}"),
            },
            Ok(r @ skstack::SkRxD::Epandesc(_)) => tracing::trace!("{r:?}"),
            Ok(skstack::SkRxD::Erxudp(erxudp)) => rx_erxudp(&pool, &settings.Unit, &erxudp).await?,
            Err(e) if e.kind() == io::ErrorKind::TimedOut => {} // タイムアウトエラーは無視する
            Err(e) => return Err(DaqDaemonError::from(e)),
        }
        tokio::task::yield_now().await;
    }
}

/// スマートメーターからデーターを収集する
async fn exec_data_acquisition(
    port_name: &str,
    database_url: &str,
) -> result::Result<(), DaqDaemonError> {
    let pool = PgPool::connect(database_url).await?;

    // データベースからスマートメーターの情報を得る
    let settings = read_settings(&pool).await?;
    let credentials = authn::Credentials {
        id: authn::Id::from_str(&settings.RouteBId).map_err(|e| DaqDaemonError::InvalidId(e))?,
        password: authn::Password::from_str(&settings.RouteBPassword)
            .map_err(|e| DaqDaemonError::InvalidPassword(e))?,
    };
    let mac_address =
        u64::from_str_radix(&settings.MacAddress, 16).or(Err(DaqDaemonError::InvalidMacAddress))?;

    // MACアドレスからIPv6リンクローカルアドレスへ変換する
    // MACアドレスの最初の1バイト下位2bit目を反転して
    // 0xFE80000000000000XXXXXXXXXXXXXXXXのXXをMACアドレスに置き換える
    let sender = Ipv6Addr::from_bits(
        0xFE80_0000_0000_0000u128 << 64 | (mac_address as u128 ^ 0x0200_0000_0000_0000u128),
    );

    // シリアルポートを開く
    let mut serial_port = serialport::new(port_name, 115200)
        .stop_bits(StopBits::One)
        .data_bits(DataBits::Eight)
        .timeout(Duration::from_secs(1))
        .open()?;

    // シリアルポート読み込みはバッファリングする
    let mut serial_port_reader = serial_port
        .try_clone()
        .and_then(|cloned| Ok(BufReader::new(cloned)))
        .or(Err(DaqDaemonError::Other("Failed to clone serial_port")))?;

    // PANA セッションライフタイム値
    const SESSION_LIFETIME_OF_SECOND: u32 = 900;

    // PANA セッション再認証間隔
    let session_rejoin_period = Duration::from_secs_f32(SESSION_LIFETIME_OF_SECOND as f32 * 0.7);

    let custom_commands = [
        format!("SKSREG S16 {:X}\r\n", SESSION_LIFETIME_OF_SECOND), // PANA セッションライフタイム値
    ];

    // スマートメーターと接続する
    authn::connect(
        &mut serial_port_reader,
        &mut serial_port,
        &credentials,
        &sender,
        settings.Channel,
        settings.PanId,
    )?;

    // 追加コマンド発行
    for command in custom_commands.iter() {
        skstack::send(&mut serial_port, command.as_bytes())?;
        thread::sleep(Duration::from_millis(1));
        if let skstack::SkRxD::Fail(code) = skstack::receive(&mut serial_port_reader)? {
            tracing::error!(
                r#"コマンド "{}" 実行に失敗しました。"#,
                command.escape_debug()
            );
            return Err(DaqDaemonError::CommandFail(code));
        }
    }

    // スマートメーター送信用スレッド
    let handle_transmitter = tokio::spawn(async move {
        smartmeter_transmitter(&sender, session_rejoin_period, &mut serial_port).await
    });

    // スマートメーター受信用スレッド
    let handle_receiver = tokio::spawn(async move {
        smartmeter_receiver(&pool, &settings, &mut serial_port_reader).await
    });

    //
    tokio::select! {
        v = handle_transmitter => v.unwrap(),
        v = handle_receiver => v.unwrap()
    }
}

/// SKSETPWD C 以降のパスワードをマスクするフォーマッタ
struct MaskingRouteBPasswordFormatter;

impl<S, N> FormatEvent<S, N> for MaskingRouteBPasswordFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        ctx: &fmt::FmtContext<'_, S, N>,
        mut writer: fmt::format::Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        // まず標準フォーマットをバッファに書き出す
        let mut buf = String::new();
        {
            let temp_writer = fmt::format::Writer::new(&mut buf);
            fmt::format::Format::default().format_event(ctx, temp_writer, event)?;
        }

        // マスク処理
        const PATTERN: &'static str = "SKSETPWD C ";
        if let Some(pos) = buf.find(PATTERN) {
            let start = pos + PATTERN.len();
            let end = (start + 12).min(buf.len() - 1);
            let masking_str = "#".repeat(end - start);
            buf.replace_range(start..end, &masking_str)
        }
        // 出力
        writer.write_str(&buf)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    // プログラムの情報
    let git_head_ref = built_info::GIT_HEAD_REF.unwrap_or_default();
    let app_info = format!(
        "{} / {}{}",
        built_info::PKG_NAME,
        built_info::PKG_VERSION,
        built_info::GIT_COMMIT_HASH_SHORT
            .map(|s| format!(" ({s} - {git_head_ref})"))
            .unwrap_or_default()
    );

    // tracingの設定
    let registry = tracing_subscriber::registry();

    // systemd-journaldに接続
    match tracing_journald::layer() {
        // journaldにログ出力する
        Ok(journald_layer) => registry.with(journald_layer).init(),
        // journaldが使えないので、標準出力にログ出力する
        Err(e) => {
            registry
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
                        .with_file(false)
                        .with_line_number(false)
                        .with_thread_names(false)
                        .with_thread_ids(false)
                        .with_ansi(false)
                        .event_format(MaskingRouteBPasswordFormatter),
                )
                .init();
            tracing::error!("couldn't connect to journald: {}", e)
        }
    }

    // このサービス本体
    let the_service_provider = async || -> result::Result<(), DaqDaemonError> {
        // 環境変数
        let serial_device = env::var("SERIAL_DEVICE")
            .map_err(|_| DaqDaemonError::Other(r#"Must be set to "SERIAL_DEVICE" environment."#))?;
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| DaqDaemonError::Other(r#"Must be set to "DATABASE_URL" environment."#))?;
        exec_data_acquisition(&serial_device, &database_url).await
    };

    // サービスを開始する
    tracing::info!("{app_info} started.");
    let reason = loop {
        break match the_service_provider().await {
            Ok(()) => {
                tokio::time::sleep(Duration::from_secs(5)).await; // 再始動まで少々クールダウン時間をもつ
                continue; // 再始動
            }
            Err(e @ DaqDaemonError::Io(_)) => e.to_string(),
            Err(e @ DaqDaemonError::BinaryEncode(_)) => e.to_string(),
            Err(e @ DaqDaemonError::Cron(_)) => e.to_string(),
            Err(e @ DaqDaemonError::OutOfRange(_)) => e.to_string(),
            Err(e @ DaqDaemonError::SerialPort(_)) => e.to_string(),
            Err(e @ DaqDaemonError::Database(_)) => e.to_string(),
            Err(e @ DaqDaemonError::InvalidId(_)) => e.to_string(),
            Err(e @ DaqDaemonError::InvalidPassword(_)) => e.to_string(),
            Err(e @ DaqDaemonError::InvalidMacAddress) => e.to_string(),
            Err(e @ DaqDaemonError::CommandFail(_)) => e.to_string(),
            Err(DaqDaemonError::PanaSessionDisconnected) => {
                tokio::time::sleep(Duration::from_secs(5)).await; // 再始動まで少々クールダウン時間をもつ
                continue; // 再始動
            }
            Err(e @ DaqDaemonError::Other(_)) => e.to_string(),
        };
    };

    // ここに到達するのは異常終了しかありえない
    tracing::error!("{app_info} aborted, reason: {reason}");
    return ExitCode::FAILURE;
}
