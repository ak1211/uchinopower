// スマートメータールートB接続
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use crate::skstack;
use std::io;
use std::net::Ipv6Addr;
use std::thread;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("i/o")]
    Io(#[from] io::Error),
    #[error("コマンド実行に失敗しました。 ER(hex) {0:X}")]
    Fail(u8),
    #[error("PANAセッションが切断された")]
    PanaSessionDisconnected,
}

#[derive(PartialEq, Eq)]
/// 認証情報
pub struct Credentials {
    pub id: Id,
    pub password: Password,
}

#[derive(PartialEq, Eq)]
/// ID
pub struct Id([char; 32]);
impl std::str::FromStr for Id {
    type Err = String;
    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.chars()
            .collect::<Vec<char>>()
            .try_into()
            .map(|a| Self(a))
            .map_err(|_| "IDは32文字固定長です".to_string())
    }
}
impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0.iter().collect::<String>())
    }
}

#[derive(PartialEq, Eq)]
/// パスワード
pub struct Password([char; 12]);
impl std::str::FromStr for Password {
    type Err = String;
    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.chars()
            .collect::<Vec<char>>()
            .try_into()
            .map(|a| Self(a))
            .map_err(|_| "PASSWORDは12文字固定長です".to_string())
    }
}
impl std::fmt::Display for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0.iter().collect::<String>())
    }
}

/// スマートメーターと接続する
pub fn connect(
    reader: &mut io::BufReader<dyn io::Read>,
    writer: &mut dyn io::Write,
    credentials: &Credentials,
    sender: &Ipv6Addr,
    channel: u8,
    pan_id: u16,
) -> std::result::Result<(), Error> {
    let sender_address = sender.segments().map(|n| format!("{:04X}", n)).join(":");

    let connect_sequence = [
        "SKRESET\r\n".to_owned(),                           // リセット
        "SKSREG SFE 0\r\n".to_owned(),                      // コマンドのエコーバックを無効にする。
        format!("SKSETPWD C {}\r\n", credentials.password), // パスワードを登録する。
        format!("SKSETRBID {}\r\n", credentials.id),        // IDを登録する。
        format!("SKSREG S2 {:02X}\r\n", channel),           // 自端末の論理チャンネル番号を設定する
        format!("SKSREG S3 {:04X}\r\n", pan_id),            // 自端末のPAN IDを設定する
        format!("SKJOIN {}\r\n", sender_address),           // PANA認証開始
    ];

    // コマンド発行
    for command in connect_sequence.iter() {
        skstack::send(writer, command.as_bytes())?;
        thread::sleep(Duration::from_millis(1));
        if let skstack::SkRxD::Fail(code) = skstack::receive(reader)? {
            return Err(Error::Fail(code));
        }
    }

    // PANA認証開始後のイベントを処理する
    loop {
        match skstack::receive(reader) {
            Ok(skstack::SkRxD::Void) => {}
            // OK
            Ok(skstack::SkRxD::Ok) => {}
            // FAIL ER
            Ok(skstack::SkRxD::Fail(code)) => return Err(Error::Fail(code)),
            // EVENT 0x24 = PANA接続失敗
            Ok(skstack::SkRxD::Event(event)) if event.code == 0x24 => {
                return Err(Error::PanaSessionDisconnected);
            }
            // EVENT 0x25 = PANA接続完了
            Ok(skstack::SkRxD::Event(event)) if event.code == 0x25 => return Ok(()),
            // 何らかのイベント
            Ok(skstack::SkRxD::Event(_event)) => continue,
            // EPANDESC
            Ok(skstack::SkRxD::Epandesc(_)) => {}
            // ERXUDP
            Ok(skstack::SkRxD::Erxudp(_)) => {}
            //
            Err(e) if e.kind() == io::ErrorKind::TimedOut => continue, // タイムアウトエラーは無視する
            //
            Err(e) => return Err(Error::Io(e)),
        }
    }
}
