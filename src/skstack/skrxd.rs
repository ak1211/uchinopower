// SKSTACK/IPの応答
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use std::net::Ipv6Addr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub code: u8,
    pub sender: std::net::Ipv6Addr,
    pub param: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Epandesc {
    pub channel: u8,
    pub channel_page: u8,
    pub pan_id: u16,
    pub addr: u64,
    pub lqi: u8,
    pub pair_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Erxudp {
    pub sender: Ipv6Addr,      // 送信元IPv6アドレス
    pub destination: Ipv6Addr, // 送信先IPv6アドレス
    pub sender_port: u16,      // 送信元UDPポート番号
    pub destination_port: u16, // 送信先UDPポート番号
    pub senderlla: u64,        // 送信元のMAC層アドレス
    pub secured: u8,           // 1:暗号化あり, 0:暗号化なし
    pub datalen: u16,          // 受信データ長
    pub data: Vec<u8>,         // 受信データ
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkRxD {
    Event(Event),       // イベント受信
    Epandesc(Epandesc), // EPANDESC受信
    Erxudp(Erxudp),     // ERXUDP受信
    Fail(u8),           // 失敗
    Ok,                 // 成功
    Void,               // 空行
}
