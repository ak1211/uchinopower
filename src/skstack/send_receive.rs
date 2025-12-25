// シリアル通信 送受信
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use crate::{
    echonetlite::EchonetliteFrame,
    skstack::{SkRxD, parser},
};
use std::io::{self, BufRead, BufReader};
use std::net::Ipv6Addr;

/// コマンドを送信する
pub fn send(w: &mut dyn io::Write, command: &[u8]) -> io::Result<()> {
    // ポートに書き込む
    let str_command = command
        .into_iter()
        .map(|n| *n as char)
        .filter(|n| n.is_ascii())
        .collect::<String>();
    tracing::trace!(target:"Tx->","{}",str_command.escape_debug());
    w.write_all(command)
}

/// 結果を受信する
pub fn receive(r: &mut BufReader<dyn io::Read>) -> io::Result<SkRxD> {
    let mut linebuf = Vec::<String>::new();
    loop {
        let mut line = String::new();
        let _ = r.read_line(&mut line)?;
        tracing::trace!(target:"<-Rx","{}", line.escape_debug());
        linebuf.push(line);
        match parser::parse_rxd(linebuf.concat().as_ref()) {
            Ok((_s, r)) => return Ok(r),
            Err(nom::Err::Incomplete(_)) => continue, // つづけて次行を読み込む
            Err(e) => tracing::trace!(target:"parser","{:?}", e),
        }
        linebuf.clear();
    }
}

/// EchonetliteフレームからSKSENDTOコマンドを作る
pub fn command_from_echonetliteframe(
    sender: &Ipv6Addr,
    frame: &EchonetliteFrame,
) -> Result<Vec<u8>, bincode::error::EncodeError> {
    let sender_address = sender.segments().map(|n| format!("{:04X}", n)).join(":");
    let config = bincode::config::standard()
        .with_big_endian()
        .with_fixed_int_encoding();
    let payload = bincode::encode_to_vec(frame, config)?;
    let sksendto = format!(
        "SKSENDTO 1 {} {:04X} 1 {:04X} ",
        sender_address,
        0x0e1a,
        payload.len(),
    );
    let command = [sksendto.as_bytes(), &payload].concat();
    Ok(command)
}
