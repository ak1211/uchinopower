// アクティブスキャンでスマートメーターを探す
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use crate::skstack::{self, authn};
use anyhow::{Context, bail};
use std::io;

/// アクティブスキャンを実行する
pub fn active_scan(
    port_reader: &mut io::BufReader<dyn io::Read>,
    port_writer: &mut dyn io::Write,
    scan_time: usize,
    credentials: &authn::Credentials,
) -> anyhow::Result<Vec<skstack::Epandesc>> {
    let pairing_sequence = [
        "SKRESET\r\n".to_owned(),                           // リセット
        format!("SKSETPWD C {}\r\n", credentials.password), // パスワードを登録する。
        format!("SKSETRBID {}\r\n", credentials.id),        // IDを登録する。
        format!("SKSCAN 2 FFFFFFFF {:X}\r\n", scan_time),   // アクティブスキャン
    ];

    // コマンド発行
    for command in pairing_sequence.iter() {
        skstack::send(port_writer, command.as_bytes()).context("write failed!")?;
        if let skstack::SkRxD::Fail(code) = skstack::receive(port_reader)? {
            bail!("\"{}\" コマンド実行に失敗しました。 ER{}", command, code);
        }
    }

    let mut found = Vec::<skstack::Epandesc>::new();
    // アクティブスキャン結果待ち
    'exit: loop {
        match skstack::receive(port_reader) {
            Ok(skstack::SkRxD::Void) => {}
            Ok(skstack::SkRxD::Ok) => {}
            Ok(fail @ skstack::SkRxD::Fail(_)) => {
                tracing::debug!("{:?}", fail);
                break;
            }
            Ok(skstack::SkRxD::Event(event)) => {
                tracing::debug!("{:?}", event);
                match event.code {
                    0x20 => continue,    // EVENT 20 = beaconを受信した
                    0x22 => break 'exit, // EVENT 22 = アクティブスキャン終了
                    _ => break 'exit,    // 何らかのイベント
                }
            }
            Ok(skstack::SkRxD::Epandesc(event)) => {
                tracing::debug!("{:?}", event);
                found.push(event);
            }
            Ok(skstack::SkRxD::Erxudp(event)) => {
                tracing::debug!("{:?}", event);
            }
            Err(e) if e.kind() == io::ErrorKind::TimedOut => continue, // タイムアウトエラーは無視する
            Err(e) => return Err(e).context("read failed!"),
        }
    }
    Ok(found)
}
