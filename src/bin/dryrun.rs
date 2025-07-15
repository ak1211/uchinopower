// スマートメータに接続してみる。
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use anyhow::{Context, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use core::time;
use serialport::{DataBits, SerialPort, StopBits};
use std::fs;
use std::fs::File;
use std::io::{self, BufReader, Write};
use std::net::Ipv6Addr;
use std::str::FromStr;
use std::sync::{LazyLock, mpsc, mpsc::TryRecvError};
use std::thread;
use std::time::Duration;
use tracing_subscriber::FmtSubscriber;
use uchinoepower::echonetlite::{
    self, EchonetliteEdata, EchonetliteFrame, smart_electric_energy_meter,
};
use uchinoepower::skstack::{self, Erxudp, authn};
use uchinoepower::{self, ConnectionSettings, pairing};

/// スマートメーターBルートから情報を取得する。
#[derive(Parser, Debug)]
#[command(name = "dryrun")]
#[command(version, about, long_about = None)]
struct Cli {
    /// 設定ファイル名
    #[arg(short = 'S', long, default_value = "uchinopower.toml")]
    config_file: String,

    /// シリアルデバイス名
    #[arg(short = 'D', long, default_value = "/dev/ttyUSB0")]
    device: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// ペアリングして情報を設定ファイルに保存する
    Pairing(PairingArgs),
    /// スマートメータから電力消費量を得る
    DryRun,
}

#[derive(Debug, Args)]
struct PairingArgs {
    /// アクティブスキャン時間(1～14)
    #[arg(short = 'T', long, default_value_t = 6)]
    activescan: usize,
    /// ルートBID(32文字)
    #[arg(long)]
    id: String,
    /// ルートBパスワード(12文字)
    #[arg(long)]
    password: String,
}

/// スマートメーターechonet lite電文
static SMARTMETER_PROPS: LazyLock<Vec<EchonetliteEdata>> = LazyLock::new(|| {
    vec![
        EchonetliteEdata {
            epc: echonetlite::superclass::GetPropertyMap::EPC, // Getプロパティマップ
            ..Default::default()
        },
        EchonetliteEdata {
            epc: echonetlite::superclass::Manufacturer::EPC, // メーカーコード
            ..Default::default()
        },
        EchonetliteEdata {
            epc: smart_electric_energy_meter::NumberOfEffectiveDigits::EPC, // 積算電力量有効桁数
            ..Default::default()
        },
    ]
});

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

/// 今日の積算電力量履歴を取得するechonet lite電文
static CWH_HISTORIES: LazyLock<EchonetliteFrame> = LazyLock::new(|| {
    EchonetliteFrame {
        ehd: 0x1081,              // 0x1081 = echonet lite
        tid: 1,                   // tid
        seoj: [0x05, 0xff, 0x01], // home controller
        deoj: [0x02, 0x88, 0x01], // smartmeter
        esv: 0x62,                // get要求
        opc: 1,                   // 1つ
        edata: vec![EchonetliteEdata {
            epc: 0xe2, // 積算電力量計測値履歴1
            ..Default::default()
        }],
    }
});

/// 積算電力量計測値を取得するechonet lite電文
static CUMLATIVE_WATT_HOUR: LazyLock<EchonetliteFrame> = LazyLock::new(|| {
    EchonetliteFrame {
        ehd: 0x1081,              // 0x1081 = echonet lite
        tid: 1,                   // tid
        seoj: [0x05, 0xff, 0x01], // home controller
        deoj: [0x02, 0x88, 0x01], // smartmeter
        esv: 0x62,                // get要求
        opc: 1,                   // 1つ
        edata: vec![EchonetliteEdata {
            epc: 0xe0, // 積算電力量計測値(正方向計測値)
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

/// 接続するスマートメーターをアクティブスキャンで探す
fn exec_pairing(cli: &Cli, args: &PairingArgs) -> anyhow::Result<()> {
    let credentials = authn::Credentials {
        id: authn::Id::from_str(&args.id).map_err(|s| anyhow!(s))?,
        password: authn::Password::from_str(&args.password).map_err(|s| anyhow!(s))?,
    };

    // シリアルポートを開く
    let mut port = open_port(&cli.device)?;

    // シリアルポート読み込みはバッファリングする
    let mut reader = port
        .try_clone()
        .and_then(|cloned| Ok(BufReader::new(cloned)))
        .expect("Failed to clone");

    // 接続するスマートメーターをアクティブスキャンで探して設定ファイルに情報を保存する
    match pairing(&mut reader, &mut port, args.activescan, &credentials)? {
        Some(settings) => {
            // TOML化
            let comment = "# uchinopower設定ファイル".to_string();
            let toml = toml::to_string_pretty(&settings)?;
            // ファイル出力
            let file_name = &cli.config_file;
            let mut file = File::create(file_name.to_owned())?;
            match file.write_all([comment, toml].join("\n").as_bytes()) {
                Ok(()) => Ok(println!("\"{}\" file write finished.", file_name)),
                Err(e) => {
                    tracing::error!("{:?}", e);
                    bail!(e);
                }
            }
        }
        None => Ok(println!("Could not find smart meter.")),
    }
}

fn exec_dryrun(cli: &Cli) -> anyhow::Result<()> {
    // 設定ファイルからスマートメーターの情報を得る
    let file = fs::read_to_string(&cli.config_file).context("setting file read error.")?;
    let settings = toml::from_str::<ConnectionSettings>(&file)?;
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
    let mut serial_port = open_port(&cli.device)?;

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

    thread::scope(|s| {
        let (tx_cancel, rx_cancel) = mpsc::channel::<()>();

        // イベント受信用スレッドを起動する
        let handle = s.spawn(move || -> anyhow::Result<()> {
            while let Err(TryRecvError::Empty) = rx_cancel.try_recv() {
                if let Some(erxudp) = take_erxudp(&mut serial_port_reader)? {
                    let config = bincode::config::standard()
                        .with_big_endian()
                        .with_fixed_int_encoding();
                    let (frame, _len): (EchonetliteFrame, usize) =
                        bincode::borrow_decode_from_slice(&erxudp.data, config).unwrap();
                    let mut s = Vec::<String>::new();
                    s.push(frame.show());
                    for v in frame.edata.iter() {
                        s.push(v.show(Some(&settings.Unit)));
                    }
                    tracing::info!("{}", s.join(" "));
                }
            }
            Ok(())
        });

        // スマートメーターの属性値を取得する
        for edata in SMARTMETER_PROPS.iter() {
            let frame = EchonetliteFrame {
                ehd: 0x1081,              // 0x1081 = echonet lite
                tid: 1,                   // tid
                seoj: [0x05, 0xff, 0x01], // home controller
                deoj: [0x02, 0x88, 0x01], // smartmeter
                esv: 0x62,                // get要求
                opc: 1,                   // 1つ
                edata: vec![edata.clone()],
            };
            skstack::send_echonetlite(&mut serial_port, &sender, &frame)?;
            thread::sleep(time::Duration::from_secs(5));
        }

        // Echonetliteメッセージ
        let elmessages: [&EchonetliteFrame; 4] = [
            &LATEST_CWH,
            &CWH_HISTORIES,
            &CUMLATIVE_WATT_HOUR,
            &INSTANT_WATT_AMPERE,
        ];

        // Echonetliteメッセージ送信
        for &msg in elmessages.iter() {
            skstack::send_echonetlite(&mut serial_port, &sender, msg)?;
            thread::sleep(time::Duration::from_secs(10));
        }

        // イベント受信用スレッドを停止する
        tx_cancel
            .send(())
            .context("Can't send cancellation signal.")?;

        // 受信用スレッドの処理結果
        match handle.join().map_err(|e| anyhow!("{:?}", e))? {
            Ok(()) => Ok(println!("Good Bye!")),
            Err(e) => bail!(e),
        }
    })
}

/// イベント受信
fn take_erxudp(serial_port_reader: &mut BufReader<dyn io::Read>) -> anyhow::Result<Option<Erxudp>> {
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
fn main() -> anyhow::Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .with_thread_names(true)
        .with_thread_ids(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let cli = Cli::parse();

    match &cli.command {
        Commands::Pairing(args) => exec_pairing(&cli, args),
        Commands::DryRun => exec_dryrun(&cli),
    }
}
